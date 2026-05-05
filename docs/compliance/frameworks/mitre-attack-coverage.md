# MITRE ATT&CK Coverage Mapping

> This document maps RavenFabric's security controls against MITRE ATT&CK
> techniques, demonstrating which attack techniques are mitigated, detected,
> or unaffected by the system's architecture.

**RavenFabric version:** v0.2-dev  
**ATT&CK version:** v14 (October 2023)  
**Matrix:** Enterprise  
**Last updated:** 2026-05-05

---

## Mapping Methodology

Each ATT&CK technique is evaluated against RavenFabric in three categories:

| Category | Meaning |
|----------|---------|
| **Mitigated** | RavenFabric's architecture prevents or significantly limits the technique |
| **Detected** | RavenFabric's audit/monitoring capabilities detect the technique |
| **Limited mitigation** | Partial protection exists but gaps remain |

---

## Initial Access (TA0001)

| Technique | ID | Coverage | RavenFabric Control |
|-----------|----|----------|---------------------|
| Valid Accounts | T1078 | Mitigated | Cryptographic identity only — no passwords, no credential reuse possible |
| Phishing | T1566 | Mitigated | No human-facing login — keys must be pre-enrolled via OTP ceremony |
| Exploit Public-Facing Application | T1190 | Limited | Relay validates magic bytes + version before handshake; minimal attack surface |
| External Remote Services | T1133 | Mitigated | All connections mutually authenticated via Noise XX; deny-by-default |

---

## Execution (TA0002)

| Technique | ID | Coverage | RavenFabric Control |
|-----------|----|----------|---------------------|
| Command and Scripting Interpreter | T1059 | Mitigated + Detected | Policy engine validates commands against allow/deny regex. All commands audit-logged |
| Native API | T1106 | N/A | Agent executes commands via `sh -c`, not native API calls |
| Exploitation for Client Execution | T1203 | Limited | Rust memory safety prevents most exploitation; no JIT, no eval |

---

## Persistence (TA0003)

| Technique | ID | Coverage | RavenFabric Control |
|-----------|----|----------|---------------------|
| Create Account | T1136 | Mitigated | No account creation API; enrollment requires pre-existing OTP (admin-generated) |
| Implant Internal Image | T1525 | N/A | Agent is a single static binary; no container images in scope |
| Valid Accounts: Default Accounts | T1078.001 | Mitigated | No default credentials exist; all keys uniquely generated |

---

## Privilege Escalation (TA0004)

| Technique | ID | Coverage | RavenFabric Control |
|-----------|----|----------|---------------------|
| Exploitation for Privilege Escalation | T1068 | Limited | Rust memory safety; agent runs as configured user, not root by default |
| Abuse Elevation Control Mechanism | T1548 | Detected | All executed commands logged; sudo/elevation attempts visible in audit |
| Access Token Manipulation | T1134 | Mitigated | No tokens — authentication is cryptographic session-based |

---

## Defense Evasion (TA0005)

| Technique | ID | Coverage | RavenFabric Control |
|-----------|----|----------|---------------------|
| Indicator Removal: Clear Logs | T1070.002 | Mitigated | Audit log is append-only (no delete/truncate API) |
| Modify Authentication Process | T1556 | Mitigated | Auth is Noise XX handshake — no pluggable modules, no PAM, no hooks |
| Impersonation | T1656 | Mitigated | Cryptographic identity — cannot impersonate without private key |
| Obfuscated Files | T1027 | Detected | Commands are plaintext-validated against policy before execution |
| Traffic Signaling | T1205 | Mitigated | Protocol magic (`RVNF`) + version required; arbitrary traffic rejected |

---

## Credential Access (TA0006)

| Technique | ID | Coverage | RavenFabric Control |
|-----------|----|----------|---------------------|
| Brute Force | T1110 | Mitigated | Rate limiting (20/IP/min); no passwords to brute force |
| Credential Dumping | T1003 | Mitigated | Keys zeroed from memory on drop; file permissions 0600 |
| Unsecured Credentials | T1552 | Mitigated | No plaintext secrets on disk; atomic file creation with permissions |
| Steal or Forge Authentication Certificates | T1649 | N/A | No PKI/certificates — raw key authentication only |
| Man-in-the-Middle | T1557 | Mitigated | Noise XX mutual authentication prevents MITM |

---

## Discovery (TA0007)

| Technique | ID | Coverage | RavenFabric Control |
|-----------|----|----------|---------------------|
| Network Service Discovery | T1046 | Limited | Relay listens on single port; minimal service fingerprint |
| System Information Discovery | T1082 | Detected | `sysinfo` commands go through policy check + audit log |
| Remote System Discovery | T1018 | Detected | Mesh topology queries are policy-controlled and logged |

---

## Lateral Movement (TA0008)

| Technique | ID | Coverage | RavenFabric Control |
|-----------|----|----------|---------------------|
| Remote Services | T1021 | Mitigated | Every hop requires separate Noise XX auth; no credential forwarding |
| Exploitation of Remote Services | T1210 | Limited | Rust memory safety; protocol strictly typed (msgpack) |
| Lateral Tool Transfer | T1570 | Detected | File operations require filesystem policy allow rules + audit |

---

## Collection (TA0009)

| Technique | ID | Coverage | RavenFabric Control |
|-----------|----|----------|---------------------|
| Data from Local System | T1005 | Mitigated + Detected | Filesystem policy restricts accessible paths; all access logged |
| Data Staged | T1074 | Detected | File writes to staging paths caught by filesystem policy |
| Clipboard Data | T1115 | N/A | No clipboard access in agent |

---

## Command and Control (TA0011)

| Technique | ID | Coverage | RavenFabric Control |
|-----------|----|----------|---------------------|
| Proxy | T1090 | Mitigated | Relay is stateless, never decrypts payload; cannot be used as open proxy |
| Encrypted Channel | T1573 | Note | RavenFabric uses encrypted channels by design — legitimate use, but also limits C2 visibility |
| Non-Standard Port | T1571 | Detected | Agent connects only to configured relay endpoint |
| Protocol Tunneling | T1572 | Mitigated | Strict protocol validation (magic, version, handshake) on all connections |
| Application Layer Protocol | T1071 | Mitigated | Custom binary protocol — cannot blend into HTTP/HTTPS |

---

## Exfiltration (TA0010)

| Technique | ID | Coverage | RavenFabric Control |
|-----------|----|----------|---------------------|
| Exfiltration Over C2 Channel | T1041 | Mitigated + Detected | Output size bounded (maxOutputBytes policy); all output bytes counted and logged |
| Exfiltration Over Alternative Protocol | T1048 | Limited | Agent cannot open arbitrary outbound connections (only relay) |
| Data Transfer Size Limits | T1030 | Note | Attacker can chunk; but output limit per-command constrains throughput |

---

## Impact (TA0040)

| Technique | ID | Coverage | RavenFabric Control |
|-----------|----|----------|---------------------|
| Data Destruction | T1485 | Mitigated | Policy deny rules (e.g., `rm -rf` blocked by default); filesystem path restrictions |
| Resource Hijacking | T1496 | Detected | Execution timeout enforced; CPU-bound commands killed after configured seconds |
| Service Stop | T1489 | Detected | All commands audited; unusual systemctl/kill commands visible |
| Disk Wipe | T1561 | Mitigated | Policy denies dangerous disk operations by default |

---

## Summary by Tactic

| Tactic | Techniques Mapped | Mitigated | Detected | Limited | N/A |
|--------|-------------------|-----------|----------|---------|-----|
| Initial Access | 4 | 3 | 0 | 1 | 0 |
| Execution | 3 | 1 | 1 | 1 | 0 |
| Persistence | 3 | 2 | 0 | 0 | 1 |
| Privilege Escalation | 3 | 2 | 1 | 0 | 0 |
| Defense Evasion | 5 | 4 | 1 | 0 | 0 |
| Credential Access | 5 | 4 | 0 | 0 | 1 |
| Discovery | 3 | 0 | 2 | 1 | 0 |
| Lateral Movement | 3 | 1 | 1 | 1 | 0 |
| Collection | 3 | 1 | 1 | 0 | 1 |
| Command and Control | 5 | 3 | 1 | 0 | 1 |
| Exfiltration | 3 | 1 | 1 | 1 | 0 |
| Impact | 4 | 2 | 2 | 0 | 0 |
| **Total** | **44** | **24** | **11** | **5** | **4** |

**Coverage: 80% of mapped techniques are mitigated or detected (35/44)**

---

## Key Architectural Advantages

1. **No passwords/tokens** — eliminates entire categories of credential attacks
2. **Deny-by-default policy** — limits blast radius of any compromise
3. **Append-only audit** — attackers cannot erase evidence
4. **Mutual authentication** — prevents impersonation and MITM
5. **Rust memory safety** — eliminates buffer overflow exploitation class
6. **Rate limiting** — prevents brute force at network layer
7. **Output bounding** — limits data exfiltration throughput
8. **Execution timeout** — prevents resource exhaustion attacks
