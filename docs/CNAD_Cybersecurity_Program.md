# Cybersecurity Program

## LN Markets S.A.S. de C.V.
### Digital Asset Service Provider (PSAD) License Application
### Submitted to: Comision Nacional de Activos Digitales (CNAD)

**Document Version:** 1.0
**Effective Date:** [Upon CNAD Approval]
**Last Review Date:** March 2026
**Next Review Date:** March 2027
**Document Owner:** Chief Technology Officer
**Approved By:** Board of Directors

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Governance Framework](#2-governance-framework)
3. [Risk Assessment Methodology](#3-risk-assessment-methodology)
4. [Asset Management](#4-asset-management)
5. [Access Control](#5-access-control)
6. [Cryptographic Controls](#6-cryptographic-controls)
7. [Network Security](#7-network-security)
8. [Application Security](#8-application-security)
9. [Operational Security](#9-operational-security)
10. [Incident Response](#10-incident-response)
11. [Business Continuity & Disaster Recovery](#11-business-continuity--disaster-recovery)
12. [Third-Party Security Management](#12-third-party-security-management)
13. [Security Awareness & Training](#13-security-awareness--training)
14. [Compliance & Audit](#14-compliance--audit)
15. [Continuous Improvement](#15-continuous-improvement)

---

## 1. Executive Summary

### 1.1 Purpose

This Cybersecurity Program establishes the policies, procedures, and controls that LN Markets S.A.S. de C.V. ("LN Markets" or "the Company") implements to protect its information systems, digital assets, and customer data from cybersecurity threats. The program ensures the confidentiality, integrity, and availability of all systems and data in compliance with El Salvador's regulatory requirements and international best practices.

### 1.2 Scope

This program applies to:
- All information systems, networks, and applications operated by LN Markets
- All digital assets under custody or management
- All employees, contractors, and third-party service providers
- All locations where LN Markets operations are conducted, including cloud infrastructure
- The Lightning Network node infrastructure and associated channels
- Integration points with hedging venues and external service providers

### 1.3 Regulatory Alignment

This Cybersecurity Program is designed to comply with:
- **Ley de Emisión de Activos Digitales (LEAD)** - Digital Assets Issuance Law of El Salvador
- **CNAD Regulatory Framework** for Digital Asset Service Providers
- **SSF (Superintendencia del Sistema Financiero)** cybersecurity guidelines
- **ISO/IEC 27001:2022** - Information Security Management Systems
- **NIST Cybersecurity Framework (CSF) 2.0**
- **SOC 2 Type II** control objectives

### 1.4 Security Principles

LN Markets' cybersecurity approach is guided by the following principles:

1. **Defense in Depth**: Multiple layers of security controls protect critical assets
2. **Least Privilege**: Access rights are limited to the minimum necessary
3. **Zero Trust**: All access requests are verified regardless of source
4. **Security by Design**: Security is integrated into all systems from inception
5. **Continuous Monitoring**: Real-time visibility into security posture
6. **Rapid Response**: Documented procedures for swift incident handling

---

## 2. Governance Framework

### 2.1 Organizational Structure

#### 2.1.1 Board of Directors
- Ultimate accountability for cybersecurity risk management
- Approves the Cybersecurity Program and material updates
- Receives quarterly cybersecurity reports
- Approves cybersecurity budget and resource allocation

#### 2.1.2 Chief Technology Officer (CTO)
- Executive responsibility for cybersecurity program implementation
- Reports cybersecurity matters to the Board
- Oversees security team and initiatives
- Approves security policies and procedures

#### 2.1.3 Security Function
- Day-to-day security operations and monitoring
- Security incident response coordination
- Vulnerability management and remediation
- Security awareness training delivery

#### 2.1.4 Local Compliance Officer (El Salvador)
- Liaison with CNAD on cybersecurity matters
- Ensures local regulatory compliance
- Coordinates security incident reporting to authorities
- Participates in security governance meetings

### 2.2 Policies and Procedures

| Document | Owner | Review Frequency |
|----------|-------|------------------|
| Cybersecurity Program | CTO | Annual |
| Information Security Policy | CTO | Annual |
| Access Control Policy | CTO | Annual |
| Incident Response Plan | CTO | Semi-annual |
| Business Continuity Plan | CTO | Annual |
| Acceptable Use Policy | CTO | Annual |
| Third-Party Security Policy | CTO | Annual |
| Data Classification Policy | CTO | Annual |
| Cryptographic Key Management Policy | CTO | Annual |

### 2.3 Risk Appetite Statement

LN Markets maintains a **low risk appetite** for cybersecurity threats that could:
- Result in loss of customer funds or digital assets
- Compromise customer personal data
- Cause extended platform unavailability (>4 hours)
- Damage regulatory standing or reputation

The Company accepts **moderate risk** for:
- Minor service disruptions (<1 hour) that do not affect funds
- Non-critical system vulnerabilities with compensating controls
- Emerging threats with low probability of exploitation

---

## 3. Risk Assessment Methodology

### 3.1 Risk Assessment Framework

LN Markets employs a structured risk assessment methodology aligned with ISO 27005 and NIST SP 800-30.

#### 3.1.1 Risk Identification
- Annual comprehensive risk assessment
- Continuous threat intelligence monitoring
- Vulnerability scanning and penetration testing results
- Incident post-mortem analysis
- Industry threat reports and advisories

#### 3.1.2 Risk Analysis

**Likelihood Scale:**
| Level | Description | Frequency |
|-------|-------------|-----------|
| 5 - Almost Certain | Expected to occur | >1 per month |
| 4 - Likely | Will probably occur | 1-12 per year |
| 3 - Possible | Might occur | 1 per 1-3 years |
| 2 - Unlikely | Could occur | 1 per 3-10 years |
| 1 - Rare | May occur in exceptional circumstances | <1 per 10 years |

**Impact Scale:**
| Level | Financial | Operational | Reputational | Regulatory |
|-------|-----------|-------------|--------------|------------|
| 5 - Critical | >$1M or >10 BTC | >24h downtime | Major media coverage | License revocation |
| 4 - Major | $100K-$1M | 4-24h downtime | Industry coverage | Formal enforcement |
| 3 - Moderate | $10K-$100K | 1-4h downtime | Social media attention | Regulatory inquiry |
| 2 - Minor | $1K-$10K | <1h downtime | Customer complaints | Warning letter |
| 1 - Insignificant | <$1K | No downtime | Isolated complaint | No action |

#### 3.1.3 Risk Evaluation Matrix

| Likelihood / Impact | Insignificant | Minor | Moderate | Major | Critical |
|---------------------|---------------|-------|----------|-------|----------|
| Almost Certain | Medium | High | High | Critical | Critical |
| Likely | Low | Medium | High | High | Critical |
| Possible | Low | Medium | Medium | High | High |
| Unlikely | Low | Low | Medium | Medium | High |
| Rare | Low | Low | Low | Medium | Medium |

### 3.2 Key Risk Register (Summary)

| Risk Category | Key Risks | Inherent Risk | Controls | Residual Risk |
|---------------|-----------|---------------|----------|---------------|
| Custody | Private key compromise | Critical | MPC custody, multi-sig, HSM | Medium |
| Custody | Unauthorized fund transfer | Critical | Multi-approval, withdrawal limits | Low |
| Platform | DDoS attack | High | CDN, rate limiting, redundancy | Low |
| Platform | Application vulnerability | High | Secure SDLC, pen testing, WAF | Medium |
| Data | Customer data breach | High | Encryption, access controls, DLP | Low |
| Lightning | Channel force-closure attack | Medium | Monitoring, watchtowers, reserves | Low |
| Third-Party | Hedging venue compromise | High | Multi-venue, exposure limits | Medium |
| Third-Party | Service provider breach | Medium | Vendor assessment, contracts | Low |

### 3.3 Risk Treatment

All identified risks are treated through one or more of:
- **Mitigation**: Implementing controls to reduce likelihood or impact
- **Transfer**: Insurance or contractual allocation to third parties
- **Acceptance**: Documented acceptance by appropriate authority
- **Avoidance**: Eliminating the risk by removing the activity

---

## 4. Asset Management

### 4.1 Asset Inventory

LN Markets maintains a comprehensive inventory of all information assets, classified by criticality and sensitivity.

#### 4.1.1 Asset Categories

| Category | Examples | Criticality |
|----------|----------|-------------|
| Digital Assets | BTC reserves, Lightning channel capacity | Critical |
| Cryptographic Keys | Private keys, API secrets, TLS certificates | Critical |
| Customer Data | KYC documents, trading history, balances | High |
| Infrastructure | Servers, databases, network equipment | High |
| Applications | Trading platform, admin systems, APIs | High |
| Business Data | Financial records, contracts, policies | Medium |

#### 4.1.2 Asset Classification

**Confidentiality Levels:**
- **Secret**: Cryptographic keys, security credentials
- **Confidential**: Customer PII, financial data, security configurations
- **Internal**: Business documents, operational procedures
- **Public**: Marketing materials, public documentation

### 4.2 Digital Asset Custody Architecture

LN Markets employs a tiered custody architecture to balance security and operational efficiency:

| Tier | Purpose | Allocation | Security Controls |
|------|---------|------------|-------------------|
| Cold Storage | Long-term reserves | 50% | Fireblocks MPC, 2-of-3 multi-sig, offline key shares |
| Warm Storage | Operational reserves | 49% | Lightning Network channels, Eclair node, monitoring |
| Hot Storage | Immediate operations | 1% | Automated limits, real-time monitoring |

#### 4.2.1 Fireblocks MPC Custody
- Multi-Party Computation eliminates single points of failure
- Key shares distributed across secure environments
- Mobile-based multi-party authorization for transactions
- No single individual can unilaterally move funds
- SOC 2 Type II certified infrastructure

#### 4.2.2 Lightning Network Operations
- Eclair node implementation following ACINQ security standards
- Approximately 34 BTC in channel capacity
- Automated channel management with security constraints
- Watchtower services for channel monitoring
- Force-closure protection mechanisms

### 4.3 Asset Lifecycle Management

All assets follow a documented lifecycle:
1. **Acquisition/Creation**: Security review and classification
2. **Operation**: Access controls, monitoring, maintenance
3. **Transfer**: Secure handoff procedures
4. **Disposal**: Secure deletion, key destruction, audit trail

---

## 5. Access Control

### 5.1 Access Control Policy

#### 5.1.1 Principles
- **Least Privilege**: Users receive minimum access required for their role
- **Separation of Duties**: Critical functions require multiple approvals
- **Need-to-Know**: Access to sensitive data requires business justification
- **Time-Limited**: Privileged access is granted for specific durations

#### 5.1.2 User Access Management

**Access Request Process:**
1. Manager submits access request with business justification
2. Security team reviews against role-based access matrix
3. Approval by system owner and security function
4. Access provisioned with audit trail
5. Quarterly access review and recertification

**Access Revocation:**
- Immediate revocation upon termination
- Same-day revocation for role changes
- Automated deprovisioning workflows
- Exit checklist includes all system access

### 5.2 Authentication Controls

#### 5.2.1 Multi-Factor Authentication (MFA)
MFA is mandatory for:
- All administrative access to production systems
- Access to custody and wallet systems
- VPN and remote access
- Cloud management consoles
- Customer accounts (optional but encouraged)

Acceptable MFA methods:
- Hardware security keys (FIDO2/WebAuthn) - preferred
- Authenticator applications (TOTP)
- Mobile push notifications

#### 5.2.2 Password Policy
- Minimum 16 characters for privileged accounts
- Minimum 12 characters for standard accounts
- Complexity requirements enforced
- Password manager required for privileged users
- No password reuse (last 24 passwords)
- Account lockout after 5 failed attempts

#### 5.2.3 Session Management
- Automatic session timeout: 15 minutes (admin), 30 minutes (standard)
- Concurrent session limits enforced
- Session tokens rotated regularly
- Secure session termination on logout

### 5.3 Privileged Access Management

#### 5.3.1 Privileged Accounts
Three designated personnel have access to cryptographic keys and critical systems:
- Each privileged user has individual, audited credentials
- Shared accounts are prohibited
- Privileged access requires additional approval
- All privileged actions are logged and monitored

#### 5.3.2 Key Personnel Access Matrix

| Function | Role 1 | Role 2 | Role 3 | Approval Required |
|----------|--------|--------|--------|-------------------|
| Hot wallet operations | Yes | Yes | No | 1-of-2 |
| Cold storage access | Yes | Yes | Yes | 2-of-3 |
| Production deployment | Yes | Yes | No | 1-of-2 + review |
| Database admin | No | Yes | Yes | 1-of-2 |
| Security configuration | Yes | No | Yes | 2-of-2 |

#### 5.3.3 Multi-Signature Requirements

| Transaction Type | Threshold | Time Delay |
|------------------|-----------|------------|
| Withdrawals < 0.1 BTC | Automated | None |
| Withdrawals 0.1 - 1 BTC | 1-of-3 | None |
| Withdrawals > 1 BTC | 2-of-3 | Manual review |
| Cold storage movement | 2-of-3 | 24-hour delay |
| Policy changes | 2-of-3 | 48-hour delay |

---

## 6. Cryptographic Controls

### 6.1 Cryptographic Policy

#### 6.1.1 Approved Algorithms

| Purpose | Algorithm | Key Size |
|---------|-----------|----------|
| Symmetric encryption | AES-256-GCM | 256 bits |
| Asymmetric encryption | RSA | 4096 bits |
| Digital signatures | ECDSA (secp256k1) | 256 bits |
| Hashing | SHA-256, SHA-3 | 256 bits |
| Key derivation | PBKDF2, Argon2 | N/A |
| TLS | TLS 1.3 (TLS 1.2 minimum) | N/A |

#### 6.1.2 Prohibited Algorithms
- MD5, SHA-1 for security purposes
- DES, 3DES
- RC4
- RSA < 2048 bits
- TLS 1.0, TLS 1.1, SSL

### 6.2 Key Management

#### 6.2.1 Key Generation
- Keys generated using cryptographically secure random number generators
- Hardware Security Modules (HSM) for critical key generation
- Key generation ceremonies documented and witnessed
- Entropy sources validated before key generation

#### 6.2.2 Key Storage
- Private keys never stored in plaintext
- MPC key shares distributed across isolated environments
- Backup keys stored in geographically separate secure facilities
- Key material encrypted at rest with separate key encryption keys

#### 6.2.3 Key Rotation

| Key Type | Rotation Frequency | Trigger Events |
|----------|-------------------|----------------|
| TLS certificates | Annual | Compromise suspected |
| API keys | Quarterly | Personnel change |
| Encryption keys | Annual | Security incident |
| Signing keys | Bi-annual | Algorithm deprecation |
| Session keys | Per-session | N/A |

#### 6.2.4 Key Destruction
- Secure key destruction procedures documented
- Cryptographic erasure for digital keys
- Physical destruction for hardware containing keys
- Destruction witnessed and documented
- Audit trail maintained for 7 years

### 6.3 Bitcoin-Specific Cryptographic Controls

#### 6.3.1 Wallet Security
- HD wallet derivation (BIP32/BIP44)
- Seed phrases encrypted and distributed
- Address generation verified against multiple implementations
- Transaction signing isolated from network-connected systems

#### 6.3.2 Lightning Network Security
- Channel keys protected by node security
- Backup of channel state maintained securely
- Watchtower integration for channel monitoring
- Force-closure procedures documented and tested

---

## 7. Network Security

### 7.1 Network Architecture

#### 7.1.1 Network Segmentation
LN Markets implements strict network segmentation:

| Zone | Purpose | Access |
|------|---------|--------|
| DMZ | Public-facing services | Internet |
| Application | Trading platform, APIs | DMZ only |
| Database | Customer data, trading records | Application only |
| Custody | Wallet systems, signing | Isolated, air-gapped where possible |
| Management | Admin systems, monitoring | VPN only |
| Lightning | Node operations, channels | Restricted peers |

#### 7.1.2 Firewall Rules
- Default deny for all traffic
- Explicit allow rules documented and reviewed quarterly
- Stateful inspection enabled
- Application-layer filtering where applicable
- Geo-blocking for high-risk jurisdictions

### 7.2 Network Protection

#### 7.2.1 DDoS Mitigation
- CDN with DDoS protection (Cloudflare/AWS Shield)
- Rate limiting at application and network layers
- Traffic analysis and anomaly detection
- Automatic scaling during attack conditions
- Incident response procedures for sustained attacks

#### 7.2.2 Intrusion Detection/Prevention
- Network-based IDS/IPS at zone boundaries
- Host-based intrusion detection on critical systems
- Signature and behavioral detection
- Real-time alerting to security team
- Automated blocking of known malicious IPs

#### 7.2.3 Traffic Encryption
- All external traffic encrypted with TLS 1.3
- Internal traffic encrypted between zones
- Lightning Network traffic uses native encryption
- VPN required for all administrative access

### 7.3 Secure Communications

#### 7.3.1 API Security
- OAuth 2.0 / API key authentication
- Rate limiting per endpoint and user
- Input validation and sanitization
- API versioning with deprecation policy
- Comprehensive API logging

#### 7.3.2 Lightning Network Communications
- Noise protocol encryption for peer communications
- Onion routing for payment privacy
- Channel peer authentication
- Gossip protocol security measures

---

## 8. Application Security

### 8.1 Secure Development Lifecycle (SDLC)

#### 8.1.1 Security Requirements
- Security requirements defined for all features
- Threat modeling for significant changes
- Privacy impact assessment for data handling
- Compliance requirements mapped to features

#### 8.1.2 Secure Coding Practices
- OWASP Top 10 vulnerabilities addressed
- Input validation on all user inputs
- Output encoding to prevent injection
- Parameterized queries for database access
- Secure session management
- Error handling without information disclosure

#### 8.1.3 Code Review
- All code changes require peer review
- Security-focused review for sensitive components
- Automated static analysis (SAST) in CI/CD pipeline
- Review checklist includes security items

#### 8.1.4 Testing
- Unit tests for security-critical functions
- Integration testing for authentication/authorization
- Dynamic application security testing (DAST)
- Penetration testing by independent third parties (annual)

### 8.2 Vulnerability Management

#### 8.2.1 Vulnerability Identification
- Automated vulnerability scanning (weekly)
- Penetration testing (annual, plus after major changes)
- Bug bounty program for external researchers
- Threat intelligence monitoring

#### 8.2.2 Vulnerability Classification

| Severity | CVSS Score | Remediation SLA |
|----------|------------|-----------------|
| Critical | 9.0 - 10.0 | 24 hours |
| High | 7.0 - 8.9 | 7 days |
| Medium | 4.0 - 6.9 | 30 days |
| Low | 0.1 - 3.9 | 90 days |

#### 8.2.3 Patch Management
- Critical patches applied within 24 hours
- Regular patch cycles (monthly)
- Testing environment for patch validation
- Rollback procedures documented
- Emergency patching procedures defined

### 8.3 Application Controls

#### 8.3.1 Trading Platform Security
- Real-time input validation
- Transaction signing verification
- Position limit enforcement
- Automated liquidation engine with failsafes
- Rate limiting to prevent abuse

#### 8.3.2 Anti-Fraud Controls
- Transaction monitoring and anomaly detection
- Velocity checks on withdrawals
- Device fingerprinting
- IP reputation scoring
- Manual review triggers for high-risk patterns

---

## 9. Operational Security

### 9.1 Security Monitoring

#### 9.1.1 Logging Requirements
All systems generate security logs including:
- Authentication events (success and failure)
- Authorization decisions
- Administrative actions
- Transaction events
- System events and errors
- Network connection events

#### 9.1.2 Log Management
- Centralized log aggregation (SIEM)
- Log retention: 2 years minimum
- Tamper-evident log storage
- Real-time log analysis and alerting
- Regular log review procedures

#### 9.1.3 Security Monitoring
- 24/7 monitoring of critical systems
- Automated alerting for security events
- Correlation of events across systems
- Escalation procedures defined
- On-call rotation for incident response

### 9.2 Operational Procedures

#### 9.2.1 Change Management
- All changes documented and approved
- Security impact assessment required
- Testing in non-production environment
- Rollback plan required
- Post-implementation review

#### 9.2.2 Configuration Management
- Baseline configurations documented
- Configuration changes tracked
- Automated configuration compliance checking
- Hardening standards applied
- Regular configuration audits

#### 9.2.3 Capacity Management
- Capacity planning and forecasting
- Performance monitoring
- Scaling procedures documented
- Resource utilization alerts

### 9.3 Data Protection

#### 9.3.1 Data at Rest
- All sensitive data encrypted (AES-256)
- Database encryption enabled
- Backup encryption with separate keys
- Secure key management for encryption keys

#### 9.3.2 Data in Transit
- TLS 1.3 for all external communications
- Internal network encryption
- API traffic encrypted
- Lightning Network native encryption

#### 9.3.3 Data Backup
- Daily backups of critical data
- Encrypted backup storage
- Geographically distributed backup locations
- Regular backup restoration testing
- Backup retention: 90 days standard, 7 years for compliance

---

## 10. Incident Response

### 10.1 Incident Response Plan

#### 10.1.1 Incident Classification

| Severity | Definition | Examples | Response Time |
|----------|------------|----------|---------------|
| Critical | Confirmed breach, fund loss, extended outage | Key compromise, theft, >4h downtime | Immediate |
| High | Likely breach, significant risk | Active attack, vulnerability exploitation | <1 hour |
| Medium | Potential security impact | Suspicious activity, minor vulnerability | <4 hours |
| Low | Minimal security impact | Policy violation, failed attack | <24 hours |

#### 10.1.2 Incident Response Team

| Role | Responsibility | Contact |
|------|----------------|---------|
| Incident Commander | Overall incident coordination | CTO |
| Technical Lead | Technical investigation and remediation | Security Lead |
| Communications Lead | Internal and external communications | CEO |
| Legal/Compliance | Regulatory notification, legal matters | Compliance Officer |
| Operations | System recovery, business continuity | Operations Lead |

### 10.2 Incident Response Phases

#### 10.2.1 Detection and Analysis
1. Alert received and logged
2. Initial triage and classification
3. Incident Commander notified (Critical/High)
4. Evidence preservation initiated
5. Scope and impact assessment

#### 10.2.2 Containment
1. Immediate containment actions
2. Short-term containment (isolate affected systems)
3. Long-term containment (patch, rebuild)
4. Evidence collection and chain of custody

#### 10.2.3 Eradication
1. Root cause identification
2. Malware/threat removal
3. Vulnerability remediation
4. System hardening

#### 10.2.4 Recovery
1. System restoration from clean backups
2. Service restoration (phased approach)
3. Enhanced monitoring during recovery
4. Verification of system integrity

#### 10.2.5 Post-Incident
1. Incident documentation
2. Lessons learned analysis
3. Control improvements identified
4. Report to management and regulators
5. Update procedures as needed

### 10.3 Regulatory Notification

#### 10.3.1 CNAD Notification
- Notify CNAD within 24 hours of confirmed security incident affecting:
  - Customer funds or digital assets
  - Customer personal data
  - Platform availability (>4 hours)
  - Regulatory compliance

#### 10.3.2 Customer Notification
- Notify affected customers without undue delay
- Provide clear information on incident and impact
- Guidance on protective actions
- Contact information for questions

### 10.4 Communication Templates

Pre-approved communication templates maintained for:
- Internal incident notification
- Customer notification
- Regulatory notification
- Media statement (if required)
- Law enforcement coordination

---

## 11. Business Continuity & Disaster Recovery

### 11.1 Business Continuity Planning

#### 11.1.1 Business Impact Analysis

| Function | RTO | RPO | Criticality |
|----------|-----|-----|-------------|
| Trading Platform | 4 hours | 1 hour | Critical |
| Custody Systems | 4 hours | 0 (no data loss) | Critical |
| Lightning Node | 2 hours | Real-time | Critical |
| Customer Support | 8 hours | 4 hours | High |
| KYC/AML Systems | 24 hours | 4 hours | High |
| Reporting Systems | 48 hours | 24 hours | Medium |

**RTO**: Recovery Time Objective
**RPO**: Recovery Point Objective

#### 11.1.2 Continuity Strategies
- Active-passive infrastructure for critical systems
- Geographic distribution of infrastructure
- Automated failover capabilities
- Manual failover procedures documented
- Regular failover testing

### 11.2 Disaster Recovery

#### 11.2.1 Recovery Scenarios

| Scenario | Strategy | Target Recovery |
|----------|----------|-----------------|
| Data center failure | Failover to secondary site | 4 hours |
| Cloud provider outage | Multi-cloud redundancy | 2 hours |
| Database corruption | Point-in-time recovery | 1 hour |
| Ransomware attack | Clean rebuild from backups | 8 hours |
| Key personnel unavailability | Cross-training, documentation | Immediate |

#### 11.2.2 Backup Strategy
- Real-time replication for databases
- Hourly snapshots for application servers
- Daily full backups
- Weekly offsite backup rotation
- Monthly backup restoration tests

#### 11.2.3 Lightning Network Recovery
- Channel backup maintenance
- Static channel backup (SCB) for force-closure recovery
- Peer connection redundancy
- Liquidity recovery procedures

### 11.3 Testing and Maintenance

| Test Type | Frequency | Scope |
|-----------|-----------|-------|
| Backup restoration | Monthly | Random sample |
| Failover test | Quarterly | Critical systems |
| Tabletop exercise | Semi-annual | All scenarios |
| Full DR test | Annual | Complete recovery |

---

## 12. Third-Party Security Management

### 12.1 Vendor Risk Assessment

#### 12.1.1 Risk Classification

| Tier | Criteria | Assessment |
|------|----------|------------|
| Critical | Access to funds, keys, or critical systems | Full security assessment, annual audit |
| High | Access to customer data or core operations | Security questionnaire, annual review |
| Medium | Limited system access | Security questionnaire, biennial review |
| Low | No system or data access | Basic due diligence |

#### 12.1.2 Critical Vendors

| Vendor | Service | Risk Tier | Security Certifications |
|--------|---------|-----------|------------------------|
| Fireblocks | MPC Custody | Critical | SOC 2 Type II, ISO 27001 |
| Sumsub | KYC/KYB, KYT (Crystal integration), Travel Rule | High | SOC 2 Type II, GDPR compliant |
| AWS/Cloud Provider | Infrastructure | Critical | SOC 2 Type II, ISO 27001 |
| Binance/Bybit/OKX | Hedging Venues | High | Varies by venue |

### 12.2 Hedging Venue Risk Management

#### 12.2.1 Exposure Limits
- Maximum 20-30% of total reserves held at hedging venues
- Per-venue exposure limits based on risk assessment
- Real-time monitoring of venue balances
- Automated rebalancing when thresholds exceeded

#### 12.2.2 Venue Monitoring
- Execution latency monitoring
- API availability tracking
- Withdrawal processing time monitoring
- Financial health indicators
- Regulatory status monitoring

#### 12.2.3 Contingency Procedures
- Automatic hedging shift to alternative venues if degradation detected
- Pre-established accounts at backup venues (Deribit, BitMEX)
- Documented procedures for venue failure scenarios
- Regular testing of venue switching procedures

### 12.3 Contractual Security Requirements

All critical and high-tier vendors must contractually agree to:
- Security incident notification within 24 hours
- Right to audit or receive audit reports
- Data protection and confidentiality obligations
- Compliance with applicable regulations
- Business continuity and disaster recovery capabilities
- Secure data destruction upon contract termination

---

## 13. Security Awareness & Training

### 13.1 Training Program

#### 13.1.1 Training Requirements

| Audience | Training | Frequency |
|----------|----------|-----------|
| All employees | Security awareness basics | Annual + onboarding |
| All employees | Phishing awareness | Quarterly |
| Developers | Secure coding practices | Annual |
| Privileged users | Advanced security, incident response | Semi-annual |
| Compliance staff | Regulatory security requirements | Annual |
| Executives | Security governance, risk management | Annual |

#### 13.1.2 Training Topics
- Information security policies and procedures
- Social engineering and phishing recognition
- Password and authentication best practices
- Data handling and classification
- Incident reporting procedures
- Physical security awareness
- Remote work security
- Cryptocurrency-specific threats

### 13.2 Awareness Activities

- Monthly security newsletters
- Simulated phishing exercises (quarterly)
- Security tips in internal communications
- Recognition for security-conscious behavior
- Security champions program

### 13.3 Training Records

- All training completion tracked
- Compliance reporting for regulatory audits
- Remedial training for failed assessments
- Training effectiveness measured annually

---

## 14. Compliance & Audit

### 14.1 Regulatory Compliance

#### 14.1.1 Compliance Monitoring
- Continuous monitoring of regulatory requirements
- Compliance calendar for reporting deadlines
- Gap analysis when regulations change
- Remediation tracking for compliance issues

#### 14.1.2 CNAD Reporting
- Periodic security reports as required
- Incident notification within required timeframes
- Annual cybersecurity program review submission
- Cooperation with regulatory examinations

### 14.2 Internal Audit

#### 14.2.1 Audit Schedule
- Annual comprehensive security audit
- Quarterly targeted audits of high-risk areas
- Ad-hoc audits based on risk assessment

#### 14.2.2 Audit Scope
- Access control effectiveness
- Change management compliance
- Incident response capability
- Backup and recovery testing
- Third-party security management
- Policy and procedure compliance

### 14.3 External Audit

#### 14.3.1 Independent Assessments
- Annual penetration testing by qualified third party
- SOC 2 Type II audit (planned)
- Regulatory examinations as required
- Certification audits (ISO 27001 target)

#### 14.3.2 Audit Findings Management
- Findings tracked in central repository
- Remediation plans with owners and deadlines
- Progress reporting to management
- Verification of remediation effectiveness

### 14.4 Metrics and Reporting

#### 14.4.1 Security Metrics

| Metric | Target | Reporting |
|--------|--------|-----------|
| Vulnerability remediation (Critical) | 100% within 24h | Weekly |
| Vulnerability remediation (High) | 100% within 7d | Weekly |
| MFA adoption | 100% privileged users | Monthly |
| Security training completion | 100% | Quarterly |
| Incident response time | <1h (Critical) | Per incident |
| Platform availability | 99.9% | Monthly |
| Phishing simulation failure rate | <5% | Quarterly |

#### 14.4.2 Board Reporting
Quarterly cybersecurity report to Board including:
- Security incident summary
- Key risk indicators
- Compliance status
- Audit findings and remediation
- Security program initiatives
- Budget utilization

---

## 15. Continuous Improvement

### 15.1 Program Review

#### 15.1.1 Annual Review
- Comprehensive program effectiveness assessment
- Alignment with business strategy
- Regulatory requirement updates
- Industry best practice comparison
- Budget and resource planning

#### 15.1.2 Trigger-Based Review
Program review initiated upon:
- Significant security incident
- Major regulatory change
- Significant business change
- Merger or acquisition
- New technology adoption

### 15.2 Improvement Initiatives

#### 15.2.1 Current Roadmap

| Initiative | Timeline | Status |
|------------|----------|--------|
| Annual penetration testing | Q2 2026 | Planned |
| Lightning withdrawal verification | Q2 2026 | In progress |
| Extended data retention (>30 days) | Q1 2026 | In progress |
| SOC 2 Type II certification | 2027 | Planned |
| ISO 27001 certification | 2027-2028 | Planned |
| Bug bounty program expansion | Q3 2026 | Planned |

### 15.3 Industry Engagement

- Participation in cryptocurrency security working groups
- Monitoring of industry threat intelligence
- Collaboration with other exchanges on security matters
- Contribution to security standards development

---

## Appendices

### Appendix A: Glossary

| Term | Definition |
|------|------------|
| BTC | Bitcoin |
| CNAD | Comision Nacional de Activos Digitales |
| DDoS | Distributed Denial of Service |
| HSM | Hardware Security Module |
| KYC | Know Your Customer |
| KYT | Know Your Transaction |
| LEAD | Ley de Emisión de Activos Digitales |
| MFA | Multi-Factor Authentication |
| MPC | Multi-Party Computation |
| RPO | Recovery Point Objective |
| RTO | Recovery Time Objective |
| SIEM | Security Information and Event Management |
| SSF | Superintendencia del Sistema Financiero |
| TLS | Transport Layer Security |

### Appendix B: Document Control

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | March 2026 | LN Markets | Initial version for CNAD submission |

### Appendix C: Related Documents

- Information Security Policy
- Access Control Policy
- Incident Response Plan
- Business Continuity Plan
- Disaster Recovery Plan
- Data Classification Policy
- Acceptable Use Policy
- Third-Party Security Policy
- Cryptographic Key Management Policy
- Physical Security Program

---

**Document Approval**

| Role | Name | Signature | Date |
|------|------|-----------|------|
| Chief Technology Officer | ______________ | ______________ | ______________ |
| Chief Executive Officer | ______________ | ______________ | ______________ |
| Board of Directors | ______________ | ______________ | ______________ |

---

*This Cybersecurity Program is a living document and will be reviewed and updated at least annually or upon significant changes to the threat landscape, regulatory requirements, or business operations.*
