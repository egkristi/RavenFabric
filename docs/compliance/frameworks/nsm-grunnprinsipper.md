# NSM Grunnprinsipper for IKT-sikkerhet 2.1 — Kartlegging

> Dette dokumentet kartlegger RavenFabric mot Nasjonal sikkerhetsmyndighets (NSM)
> grunnprinsipper for IKT-sikkerhet, versjon 2.1.

**RavenFabric-versjon:** v0.3.0  
**Standard:** NSM Grunnprinsipper for IKT-sikkerhet 2.1  
**Sist oppdatert:** 2026-05-10

---

## Om kartleggingen

NSM Grunnprinsipper er det mest brukte rammeverket i norsk offentlig sektor
for å etablere og vedlikeholde et forsvarlig IKT-sikkerhetsnivå. RavenFabric
adresserer primært kategoriene **Beskytte** og **Oppdage**, samt deler av
**Identifisere** og **Håndtere og gjenopprette**.

---

## Kategori 1: Identifisere og kartlegge

### 1.1 Kartlegg enheter og programvare

| Prinsipp | RavenFabric-implementasjon |
|----------|----------------------------|
| Ha oversikt over alle enheter | Agenter rapporterer systeminfo (CPU, minne, disk) via `Action::Metrics`. Prometheus `/metrics` gir kontinuerlig synlighet. |
| Ha oversikt over programvare | Agenter kan kjøre inventar-kommandoer under policy-kontroll. SBOM genereres for selve RavenFabric. |

### 1.2 Kartlegg sårbarhet

| Prinsipp | RavenFabric-implementasjon |
|----------|----------------------------|
| Kartlegg sårbarheter | Dependabot-varsler og CodeQL-skanning i CI. Helsesjekk-prober oppdager tjenestefeil. |
| Prioriter tiltak basert på risiko | Policy-motoren muliggjør risikostyrt tilgangskontroll — strengere regler for kritiske systemer. |

### 1.3 Kartlegg brukere og tilganger

| Prinsipp | RavenFabric-implementasjon |
|----------|----------------------------|
| Ha oversikt over hvem som har tilgang | Trust store med agent-ID → offentlig nøkkel-mapping. Alle tilkoblinger autentiseres kryptografisk. |
| Begrens tilganger til det nødvendige | Deny-by-default policy-motor. Kun eksplisitt tillatte kommandoer og filstier. |

---

## Kategori 2: Beskytte og opprettholde

### 2.1 Ivareta sikker konfigurasjon

| Prinsipp | RavenFabric-implementasjon |
|----------|----------------------------|
| Etabler en sikker standardkonfigurasjon | Deny-by-default policy. Ingen åpne porter som standard. Kryptering er obligatorisk (ingen ukryptert modus). |
| Endre standardpassord | Ingen passord i systemet — kryptografisk identitet via Noise XX nøkkelpar. |
| Beskytt mot uautoriserte endringer | Policy-regler med regex-mønstre. Filsystem-deny-regler forhindrer skriving til kritiske stier. |
| Aktiver automatisk oppdatering der det er mulig | Agenter kan oppdateres via `rf exec` med policy-kontrollerte kommandoer. |

### 2.2 Beskytt mot ondsinnet kode

| Prinsipp | RavenFabric-implementasjon |
|----------|----------------------------|
| Aktiver sikkerhetsmekanismer | Rust-minnebeskyttelse eliminerer hele klasser av sårbarheter (buffer overflow, use-after-free). Alle RPC-handlinger er policy-kontrollerte. |
| Kontroller dataflyt | Policy-motoren kontrollerer hvilke kommandoer som kan kjøres, hvilke filer som kan leses/skrives, og ressursgrenser (output, timeout). |

### 2.3 Beskytt nettverket

| Prinsipp | RavenFabric-implementasjon |
|----------|----------------------------|
| Krypter kommunikasjon | All kommunikasjon bruker Noise XX (ChaCha20-Poly1305 AEAD). Ende-til-ende — relay ser kun kryptert data. |
| Beskytt trådløse nettverk | Nettverksposisjon er irrelevant — zero trust. Same kryptering og autentisering uavhengig av transportlag. |
| Segmenter nettverk | Transport-agnostisk arkitektur. Agenter kan nås via ulike transportlag (WebSocket, QUIC, WireGuard) med separate policy-regler per segment. |

### 2.4 Kontroller tilgang til data og tjenester

| Prinsipp | RavenFabric-implementasjon |
|----------|----------------------------|
| Etabler sterk autentisering | Noise XX gjensidig autentisering (begge parter verifiserer hverandre kryptografisk). OTP for førstegangsenrollering (engangs, tidsbegrenset). |
| Etabler tilgangskontroll | Deny-by-default policy med eksplisitte allow-regler per kommando-mønster og filsti. Dobbel sjekk: kontroller + agent. |
| Minimer tilganger (least privilege) | Policy definerer nøyaktig hvilke kommandoer som er tillatt (regex-mønster). Alt annet er blokkert. |
| Beskytt data i hvile | Privatnøkler lagres med 0600-tillatelser. Nøkler nullstilles fra minnet ved `Drop`. Audit-log er append-only. |

### 2.5 Beskytt e-post og nettleser

Ikke direkte relevant for RavenFabric (verken e-post- eller nettlesersystem).

### 2.6 Beskytt tjenester og applikasjoner

| Prinsipp | RavenFabric-implementasjon |
|----------|----------------------------|
| Minimer eksponert angrepsflate | Ingen lytteporter på agenter — de kobler *ut* til relay. Rate-limiting (20 tilkoblinger/IP/min) på relay. |
| Beskytt mot kjente sårbarheter | Dependabot, CodeQL, clippy med `-Dwarnings`, 1,179 tester. Rust eliminerer minnefeil. |
| Valider all inndata | RPC-meldinger deserialiseres via msgpack med typevalidering. Wire-protokoll krever magic + versjon. Policy validerer kommandostrenger mot regex. |

---

## Kategori 3: Oppdage

### 3.1 Overvåk og oppdage avvik

| Prinsipp | RavenFabric-implementasjon |
|----------|----------------------------|
| Etabler sikkerhetsovervåking | Strukturert audit-logg (JSON-lines) med alle beslutninger. Prometheus `/metrics` for kontinuerlig overvåking. |
| Analyser hendelser | Audit-logg inkluderer: tidsstempel, anroper-nøkkel, handling, kommando, beslutning, matchet regel, varighet, exitkode. |
| Oppdage avvik i nettverkskommunikasjon | Tamper-deteksjon: MAC-feil og latens-anomalier oppdages automatisk. `HeartbeatStatus::LatencyAnomaly`. |
| Oppdag uautorisert bruk | Alle avslåtte forespørsler logges med anroper-identitet. Policy-brudd er synlige i audit-loggen. |

### 3.2 Vurder sikkerhetsdata

| Prinsipp | RavenFabric-implementasjon |
|----------|----------------------------|
| Korreler sikkerhetsdata | JSON-lines audit-format er maskinlesbart — kan mates inn i SIEM (Splunk, ELK, Wazuh). |
| Ha oversikt over normal aktivitet | Baseline-tracking via `RttTracker` (EWMA). Helsesjekk-prober definerer "normaltilstand". |

---

## Kategori 4: Håndtere og gjenopprette

### 4.1 Forbered håndtering av hendelser

| Prinsipp | RavenFabric-implementasjon |
|----------|----------------------------|
| Planlegg hendelseshåndtering | Automatisk failover via `ConnectionManager`. DTN-kø sikrer meldinger under nedetid. |
| Vurder etablering av response-team | Audit-logg gir fullstendig forensisk spor for etterforskning. |

### 4.2 Håndter hendelser

| Prinsipp | RavenFabric-implementasjon |
|----------|----------------------------|
| Begrens konsekvenser | Policy isolerer hva en kompromittert sesjon kan gjøre. Timeout og output-begrensning forhindrer ressursutmattelse. |
| Sørg for autonome sikkerhetsmekanismer | Tamper-deteksjon trigger automatisk transportmigrering — kompromitterte stier forlates umiddelbart uten operatør-inngrep. |

### 4.3 Gjenopprett og lær

| Prinsipp | RavenFabric-implementasjon |
|----------|----------------------------|
| Sørg for gjenoppretting | Agenter rekobler automatisk med eksponentiell backoff + jitter. DTN-kø bevarer kommandoer gjennom nedetid. |
| Gjennomfør evalueringer | Fullstendig audit-spor muliggjør post-mortem-analyse. Alle beslutninger er sporbare til policy-regel. |

---

## Oppsummering av dekning

| Kategori | Dekning | Kommentar |
|----------|---------|-----------|
| 1. Identifisere | **Delvis** | Systeminfo via agenter, men ingen formell CMDB-integrasjon |
| 2. Beskytte | **God** | Kryptering, tilgangskontroll, policy, least privilege — kjerneegenskaper |
| 3. Oppdage | **God** | Audit, tamper-deteksjon, anomali-oppdagelse |
| 4. Håndtere/Gjenopprette | **Delvis** | Automatisk failover og gjenoppretting, men ingen formell IR-prosess |

---

## Mangler og plan

| Gap | NSM-prinsipp | Planlagt utbedring | Status |
|-----|--------------|---------------------|--------|
| ~~Ingen CMDB-integrasjon~~ | 1.1 Kartlegg enheter | Agent auto-registrering | Done — `AgentRegistry` med heartbeat, label-seleksjon, grains |
| ~~Ingen SIEM-eksport~~ | 3.1 Overvåk | OTLP JSON-eksport | Done — `TraceContext` (W3C traceparent), OTLP span-eksport, Prometheus metrics |
| Ingen MFA for operatører | 2.4 Sterk autentisering | WebAuthn/FIDO2 | Planlagt |
| Ingen formell IR-prosess | 4.1 Planlegg håndtering | Dokumentasjon + runbooks | Planlagt |
| ~~Ingen sikkerhetsbaseline-drift~~ | 1.2 Kartlegg sårbarhet | Desired-state med drift-deteksjon | Done — `ConvergenceEngine` med drift-rapport og auto-remediering |
