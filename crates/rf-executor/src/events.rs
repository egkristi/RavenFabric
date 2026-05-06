//! Event system for trigger-based execution.
//!
//! Provides scheduled (cron), file-watch, process-exit, and webhook triggers
//! that can fire actions when conditions are met.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};

/// Maximum number of events buffered in the broadcast channel.
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// An event trigger definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum EventTrigger {
    /// Cron-scheduled trigger.
    Cron {
        name: String,
        schedule: String,
        action: Action,
    },
    /// File system watch trigger.
    FileWatch {
        name: String,
        paths: Vec<String>,
        #[serde(default)]
        events: Vec<FileEvent>,
        action: Action,
    },
    /// Process exit trigger.
    ProcessExit {
        name: String,
        process: String,
        #[serde(default)]
        on_exit_code: Option<i32>,
        action: Action,
    },
    /// Webhook trigger (HTTP endpoint fires event).
    Webhook {
        name: String,
        #[serde(default)]
        secret: Option<String>,
        action: Action,
    },
    /// Timer trigger (fires after a delay).
    Timer {
        name: String,
        interval_seconds: u64,
        #[serde(default)]
        repeat: bool,
        action: Action,
    },
}

impl EventTrigger {
    /// Get the trigger name.
    pub fn name(&self) -> &str {
        match self {
            Self::Cron { name, .. }
            | Self::FileWatch { name, .. }
            | Self::ProcessExit { name, .. }
            | Self::Webhook { name, .. }
            | Self::Timer { name, .. } => name,
        }
    }
}

/// File system event types to watch for.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FileEvent {
    Create,
    Modify,
    Delete,
    Rename,
}

/// Action to execute when a trigger fires.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Action {
    /// Execute a command.
    Exec { command: String },
    /// Run a desired-state convergence check.
    Converge { spec: String },
    /// Send a webhook notification.
    Notify { url: String, payload: Option<String> },
}

/// A fired event with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub trigger_name: String,
    pub trigger_type: String,
    pub timestamp: String,
    pub metadata: HashMap<String, String>,
    pub action: Action,
}

impl Event {
    /// Create a new event from a trigger firing.
    pub fn from_trigger(trigger: &EventTrigger, metadata: HashMap<String, String>) -> Self {
        let (trigger_name, trigger_type, action) = match trigger {
            EventTrigger::Cron { name, action, .. } => (name.clone(), "cron".to_string(), action.clone()),
            EventTrigger::FileWatch { name, action, .. } => {
                (name.clone(), "file_watch".to_string(), action.clone())
            }
            EventTrigger::ProcessExit { name, action, .. } => {
                (name.clone(), "process_exit".to_string(), action.clone())
            }
            EventTrigger::Webhook { name, action, .. } => {
                (name.clone(), "webhook".to_string(), action.clone())
            }
            EventTrigger::Timer { name, action, .. } => {
                (name.clone(), "timer".to_string(), action.clone())
            }
        };

        Self {
            trigger_name,
            trigger_type,
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata,
            action,
        }
    }
}

/// Event bus for publishing and subscribing to events.
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<Event>,
    triggers: Arc<RwLock<Vec<EventTrigger>>>,
}

impl EventBus {
    /// Create a new event bus.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            sender,
            triggers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a trigger with the event bus.
    pub async fn register_trigger(&self, trigger: EventTrigger) {
        info!(trigger = %trigger.name(), "registering event trigger");
        self.triggers.write().await.push(trigger);
    }

    /// Remove a trigger by name.
    pub async fn remove_trigger(&self, name: &str) -> bool {
        let mut triggers = self.triggers.write().await;
        let before = triggers.len();
        triggers.retain(|t| t.name() != name);
        let removed = triggers.len() < before;
        if removed {
            info!(trigger = %name, "removed event trigger");
        }
        removed
    }

    /// List all registered triggers.
    pub async fn list_triggers(&self) -> Vec<EventTrigger> {
        self.triggers.read().await.clone()
    }

    /// Fire an event manually (e.g., from a webhook endpoint or file watcher).
    pub fn fire(&self, event: Event) -> Result<(), EventError> {
        match self.sender.send(event.clone()) {
            Ok(n) => {
                info!(
                    trigger = %event.trigger_name,
                    subscribers = n,
                    "event fired"
                );
                Ok(())
            }
            Err(_) => {
                warn!(trigger = %event.trigger_name, "no subscribers for event");
                // Not a hard error — events can fire without subscribers
                Ok(())
            }
        }
    }

    /// Subscribe to events. Returns a receiver that gets all fired events.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// Get the number of registered triggers.
    pub async fn trigger_count(&self) -> usize {
        self.triggers.read().await.len()
    }

    /// Fire a trigger by name with the given metadata.
    pub async fn fire_trigger(
        &self,
        name: &str,
        metadata: HashMap<String, String>,
    ) -> Result<(), EventError> {
        let triggers = self.triggers.read().await;
        let trigger = triggers
            .iter()
            .find(|t| t.name() == name)
            .ok_or_else(|| EventError::TriggerNotFound(name.to_string()))?;

        let event = Event::from_trigger(trigger, metadata);
        drop(triggers); // Release lock before firing
        self.fire(event)
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors from the event system.
#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("trigger not found: {0}")]
    TriggerNotFound(String),
    #[error("invalid schedule: {0}")]
    InvalidSchedule(String),
}

/// A simple timer-based scheduler that fires triggers at their configured intervals.
pub struct TimerScheduler {
    bus: EventBus,
    handles: Arc<RwLock<Vec<tokio::task::JoinHandle<()>>>>,
}

impl TimerScheduler {
    /// Create a new timer scheduler attached to an event bus.
    pub fn new(bus: EventBus) -> Self {
        Self {
            bus,
            handles: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Start a timer trigger. Returns immediately; the timer runs in the background.
    pub async fn start_timer(&self, trigger: EventTrigger) -> Result<(), EventError> {
        let (name, interval, repeat, action) = match &trigger {
            EventTrigger::Timer {
                name,
                interval_seconds,
                repeat,
                action,
            } => (name.clone(), *interval_seconds, *repeat, action.clone()),
            _ => return Ok(()), // Not a timer trigger, ignore
        };

        let bus = self.bus.clone();
        bus.register_trigger(trigger).await;

        let handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;

                let event = Event {
                    trigger_name: name.clone(),
                    trigger_type: "timer".to_string(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    metadata: HashMap::new(),
                    action: action.clone(),
                };

                if bus.fire(event).is_err() {
                    break;
                }

                if !repeat {
                    break;
                }
            }
        });

        self.handles.write().await.push(handle);
        Ok(())
    }

    /// Cancel all running timers.
    pub async fn cancel_all(&self) {
        let mut handles = self.handles.write().await;
        for handle in handles.drain(..) {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cron_trigger() {
        let yaml = r#"
type: cron
name: hourly-check
schedule: "0 * * * *"
action:
  type: exec
  command: "/usr/local/bin/check-health"
"#;
        let trigger: EventTrigger = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(trigger.name(), "hourly-check");
        if let EventTrigger::Cron { schedule, action, .. } = &trigger {
            assert_eq!(schedule, "0 * * * *");
            assert!(matches!(action, Action::Exec { .. }));
        } else {
            panic!("expected Cron trigger");
        }
    }

    #[test]
    fn test_parse_filewatch_trigger() {
        let yaml = r#"
type: filewatch
name: config-reload
paths:
  - /etc/nginx/nginx.conf
  - /etc/nginx/conf.d/
events:
  - modify
  - create
action:
  type: exec
  command: "systemctl reload nginx"
"#;
        let trigger: EventTrigger = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(trigger.name(), "config-reload");
        if let EventTrigger::FileWatch { paths, events, .. } = &trigger {
            assert_eq!(paths.len(), 2);
            assert_eq!(events.len(), 2);
            assert!(events.contains(&FileEvent::Modify));
        } else {
            panic!("expected FileWatch trigger");
        }
    }

    #[test]
    fn test_parse_process_exit_trigger() {
        let yaml = r#"
type: processexit
name: restart-on-crash
process: my-daemon
on_exit_code: 1
action:
  type: exec
  command: "systemctl restart my-daemon"
"#;
        let trigger: EventTrigger = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(trigger.name(), "restart-on-crash");
        if let EventTrigger::ProcessExit {
            process,
            on_exit_code,
            ..
        } = &trigger
        {
            assert_eq!(process, "my-daemon");
            assert_eq!(*on_exit_code, Some(1));
        } else {
            panic!("expected ProcessExit trigger");
        }
    }

    #[test]
    fn test_parse_webhook_trigger() {
        let yaml = r#"
type: webhook
name: deploy-hook
secret: "abc123"
action:
  type: converge
  spec: web-server-baseline
"#;
        let trigger: EventTrigger = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(trigger.name(), "deploy-hook");
        if let EventTrigger::Webhook { secret, action, .. } = &trigger {
            assert_eq!(secret.as_deref(), Some("abc123"));
            assert!(matches!(action, Action::Converge { .. }));
        } else {
            panic!("expected Webhook trigger");
        }
    }

    #[test]
    fn test_parse_timer_trigger() {
        let yaml = r#"
type: timer
name: periodic-sync
interval_seconds: 60
repeat: true
action:
  type: exec
  command: "rsync -a /data /backup"
"#;
        let trigger: EventTrigger = serde_yaml::from_str(yaml).unwrap();
        if let EventTrigger::Timer {
            interval_seconds,
            repeat,
            ..
        } = &trigger
        {
            assert_eq!(*interval_seconds, 60);
            assert!(*repeat);
        } else {
            panic!("expected Timer trigger");
        }
    }

    #[tokio::test]
    async fn test_event_bus_register_and_list() {
        let bus = EventBus::new();
        assert_eq!(bus.trigger_count().await, 0);

        let trigger = EventTrigger::Timer {
            name: "test".into(),
            interval_seconds: 60,
            repeat: false,
            action: Action::Exec {
                command: "echo hi".into(),
            },
        };

        bus.register_trigger(trigger).await;
        assert_eq!(bus.trigger_count().await, 1);

        let triggers = bus.list_triggers().await;
        assert_eq!(triggers[0].name(), "test");
    }

    #[tokio::test]
    async fn test_event_bus_remove_trigger() {
        let bus = EventBus::new();
        let trigger = EventTrigger::Timer {
            name: "removable".into(),
            interval_seconds: 10,
            repeat: false,
            action: Action::Exec {
                command: "true".into(),
            },
        };

        bus.register_trigger(trigger).await;
        assert_eq!(bus.trigger_count().await, 1);

        assert!(bus.remove_trigger("removable").await);
        assert_eq!(bus.trigger_count().await, 0);

        // Removing non-existent returns false
        assert!(!bus.remove_trigger("nonexistent").await);
    }

    #[tokio::test]
    async fn test_event_bus_fire_and_receive() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        let trigger = EventTrigger::Timer {
            name: "fire-test".into(),
            interval_seconds: 10,
            repeat: false,
            action: Action::Exec {
                command: "echo fired".into(),
            },
        };
        bus.register_trigger(trigger).await;

        bus.fire_trigger("fire-test", HashMap::new()).await.unwrap();

        let event = rx.recv().await.unwrap();
        assert_eq!(event.trigger_name, "fire-test");
        assert_eq!(event.trigger_type, "timer");
        assert!(matches!(event.action, Action::Exec { .. }));
    }

    #[tokio::test]
    async fn test_event_bus_fire_trigger_not_found() {
        let bus = EventBus::new();
        let result = bus.fire_trigger("nonexistent", HashMap::new()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), EventError::TriggerNotFound(_)));
    }

    #[tokio::test]
    async fn test_timer_scheduler_fires() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        let scheduler = TimerScheduler::new(bus.clone());

        let trigger = EventTrigger::Timer {
            name: "fast-timer".into(),
            interval_seconds: 1,
            repeat: false,
            action: Action::Exec {
                command: "echo tick".into(),
            },
        };

        scheduler.start_timer(trigger).await.unwrap();

        // Wait for the timer to fire
        let event = tokio::time::timeout(
            tokio::time::Duration::from_secs(3),
            rx.recv(),
        )
        .await
        .expect("timer should fire within 3s")
        .unwrap();

        assert_eq!(event.trigger_name, "fast-timer");
        scheduler.cancel_all().await;
    }

    #[test]
    fn test_event_from_trigger() {
        let trigger = EventTrigger::Webhook {
            name: "deploy".into(),
            secret: None,
            action: Action::Converge {
                spec: "baseline".into(),
            },
        };

        let mut meta = HashMap::new();
        meta.insert("source_ip".into(), "10.0.0.1".into());

        let event = Event::from_trigger(&trigger, meta);
        assert_eq!(event.trigger_name, "deploy");
        assert_eq!(event.trigger_type, "webhook");
        assert_eq!(event.metadata.get("source_ip").unwrap(), "10.0.0.1");
    }

    #[test]
    fn test_event_serialization() {
        let event = Event {
            trigger_name: "test".into(),
            trigger_type: "timer".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            metadata: HashMap::new(),
            action: Action::Exec {
                command: "echo hi".into(),
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        let deser: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.trigger_name, "test");
    }
}
