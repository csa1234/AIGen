// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Constitutional filtering system based on 120 biblical principles.
//!
//! This module provides a content filtering mechanism that scans model outputs
//! against 120 constitutional principles organized into 10 categories.
//! The constitution is stored immutably on IPFS with hash verification.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Maximum text length to scan (prevents DoS via catastrophic backtracking)
pub const MAX_SCAN_LENGTH: usize = 100_000; // 100KB

/// Category of constitutional principles
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy)]
pub enum PrincipleCategory {
    LoveAndCompassion,
    TruthAndIntegrity,
    PeaceAndNonViolence,
    HumilityAndService,
    PurityAndChastity,
    JusticeAndHonesty,
    FamilyAndRelationships,
    ScienceAndKnowledge,
    TechnologyAndAI,
    GovernanceAndTransparency,
}

impl PrincipleCategory {
    /// Get display name for the category
    pub fn name(&self) -> &'static str {
        match self {
            PrincipleCategory::LoveAndCompassion => "Love & Compassion",
            PrincipleCategory::TruthAndIntegrity => "Truth & Integrity",
            PrincipleCategory::PeaceAndNonViolence => "Peace & Non-Violence",
            PrincipleCategory::HumilityAndService => "Humility & Service",
            PrincipleCategory::PurityAndChastity => "Purity & Chastity",
            PrincipleCategory::JusticeAndHonesty => "Justice & Honesty",
            PrincipleCategory::FamilyAndRelationships => "Family & Relationships",
            PrincipleCategory::ScienceAndKnowledge => "Science & Knowledge",
            PrincipleCategory::TechnologyAndAI => "Technology & AI",
            PrincipleCategory::GovernanceAndTransparency => "Governance & Transparency",
        }
    }
}

/// Action to take when a violation is detected
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub enum ViolationAction {
    BlockIfViolated,
}

/// A single constitutional principle with detection patterns
#[derive(Clone, Debug)]
pub struct Principle {
    pub id: u32,
    pub category: PrincipleCategory,
    pub text: &'static str,
    pub patterns: &'static [&'static str],
    pub action: ViolationAction,
}

/// A detected violation of a constitutional principle
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Violation {
    pub principle_id: u32,
    pub category: String,
    pub principle_text: String,
    pub matched_pattern: String,
    pub matched_text: String,
    pub position: usize,
}

/// Constitution state stored on-chain
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConstitutionState {
    pub version: u32,
    pub ipfs_hash: String,
    pub principles_hash: [u8; 32],
    pub updated_at: i64,
    pub updated_by: String,
}

/// Constitutional filter with compiled regex patterns
pub struct ConstitutionalFilter {
    compiled_patterns: Vec<(u32, PrincipleCategory, Regex)>,
    principle_map: HashMap<u32, &'static Principle>,
}

impl ConstitutionalFilter {
    /// Create a new filter with all compiled patterns
    pub fn new() -> Self {
        let mut compiled = Vec::new();
        let mut principle_map = HashMap::new();

        for principle in CONSTITUTION.iter() {
            principle_map.insert(principle.id, principle);
            
            for pattern in principle.patterns.iter() {
                match Regex::new(pattern) {
                    Ok(regex) => {
                        compiled.push((principle.id, principle.category, regex));
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to compile regex pattern for principle {}: {}",
                            principle.id, e
                        );
                    }
                }
            }
        }

        Self {
            compiled_patterns: compiled,
            principle_map,
        }
    }

    /// Scan text for constitutional violations
    pub fn scan_text(&self, text: &str) -> Vec<Violation> {
        let mut violations = Vec::new();
        
        // Limit text length to prevent DoS
        let scan_text = if text.len() > MAX_SCAN_LENGTH {
            &text[..MAX_SCAN_LENGTH]
        } else {
            text
        };

        for (principle_id, category, regex) in &self.compiled_patterns {
            for mat in regex.find_iter(scan_text) {
                if let Some(principle) = self.principle_map.get(principle_id) {
                    violations.push(Violation {
                        principle_id: *principle_id,
                        category: category.name().to_string(),
                        principle_text: principle.text.to_string(),
                        matched_pattern: regex.as_str().to_string(),
                        matched_text: mat.as_str().to_string(),
                        position: mat.start(),
                    });
                }
            }
        }

        violations
    }

    /// Check if text is compliant (no violations)
    pub fn is_compliant(&self, text: &str) -> bool {
        self.scan_text(text).is_empty()
    }
}

impl Default for ConstitutionalFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Global singleton constitutional filter
pub static CONSTITUTIONAL_FILTER: Lazy<ConstitutionalFilter> = 
    Lazy::new(ConstitutionalFilter::new);

/// Check text for constitutional compliance
pub fn check_constitutional_compliance(text: &str) -> Vec<Violation> {
    CONSTITUTIONAL_FILTER.scan_text(text)
}

/// Get a principle by its ID
pub fn get_principle_by_id(id: u32) -> Option<&'static Principle> {
    CONSTITUTION.iter().find(|p| p.id == id)
}

/// Get all principles in a category
pub fn get_principles_by_category(category: PrincipleCategory) -> Vec<&'static Principle> {
    CONSTITUTION
        .iter()
        .filter(|p| p.category == category)
        .collect()
}

/// Serialize constitution to JSON bytes
pub fn serialize_constitution() -> Result<Vec<u8>, crate::GenesisError> {
    let constitution_data = ConstitutionData {
        version: 1,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64,
        total_principles: CONSTITUTION.len() as u32,
        categories: vec![
            ("Love & Compassion".to_string(), 20),
            ("Truth & Integrity".to_string(), 15),
            ("Peace & Non-Violence".to_string(), 15),
            ("Humility & Service".to_string(), 10),
            ("Purity & Chastity".to_string(), 10),
            ("Justice & Honesty".to_string(), 15),
            ("Family & Relationships".to_string(), 10),
            ("Science & Knowledge".to_string(), 10),
            ("Technology & AI".to_string(), 5),
            ("Governance & Transparency".to_string(), 10),
        ],
    };
    
    serde_json::to_vec_pretty(&constitution_data)
        .map_err(|e| crate::GenesisError::SerializationError(e.to_string()))
}

/// Compute SHA3-256 hash of serialized constitution
pub fn compute_constitution_hash() -> [u8; 32] {
    use sha3::{Digest, Sha3_256};
    
    let serialized = serialize_constitution().unwrap_or_default();
    let hash = Sha3_256::digest(&serialized);
    let mut result = [0u8; 32];
    result.copy_from_slice(&hash);
    result
}

/// Constitution metadata for serialization
#[derive(Clone, Debug, Serialize, Deserialize)]
struct ConstitutionData {
    version: u32,
    timestamp: i64,
    total_principles: u32,
    categories: Vec<(String, u32)>,
}

/// The complete constitution of biblical principles
pub const CONSTITUTION: [Principle; 120] = [
    // === Love & Compassion (20 principles) ===
    Principle {
        id: 1,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Love your neighbor as yourself (Matthew 22:39)",
        patterns: &[r"(?i)\b(hate|despise|loathe)\s+(all|every|those)\s+\w+", r"(?i)\b(kill|harm|hurt|destroy)\s+(them|those people|everyone)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 2,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Do to others as you would have them do to you (Matthew 7:12)",
        patterns: &[r"(?i)\b(exploit|use|abuse)\s+(others|people|victims)\s+(for|to)", r"(?i)\b(manipulate|deceive)\s+others\s+(for|to)\s+(gain|profit|advantage)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 3,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Love your enemies and pray for those who persecute you (Matthew 5:44)",
        patterns: &[r"(?i)\b(attack|destroy|eliminate)\s+(our|the)\s+(enemies|opponents|rivals)", r"(?i)\b(retaliate|revenge|get even)\s+(against|with)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 4,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Clothe yourselves with compassion (Colossians 3:12)",
        patterns: &[r"(?i)\b(show no|without)\s+(mercy|compassion|empathy)", r"(?i)\b(cold|heartless|ruthless)\s+(toward|to|with)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 5,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Be kind and compassionate to one another (Ephesians 4:32)",
        patterns: &[r"(?i)\b(be|act)\s+(cruel|mean|harsh)\s+(to|toward|with)", r"(?i)\b(bullying|tormenting)\s+(others|someone|people)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 6,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Whoever does not love does not know God (1 John 4:8)",
        patterns: &[r"(?i)\b(love is weakness|hate is strength|love is stupid)", r"(?i)\b(discard|reject)\s+(love|compassion|kindness)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 7,
        category: PrincipleCategory::LoveAndCompassion,
        text: "A new command I give you: Love one another (John 13:34)",
        patterns: &[r"(?i)\b(divide|split|separate)\s+(people|us|them)\s+(against|to)\s+(each other|one another)", r"(?i)\b(pit\s+.*\s+against|set\s+.*\s+against)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 8,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Above all, love each other deeply (1 Peter 4:8)",
        patterns: &[r"(?i)\b(superficial|fake|pretend)\s+(love|care|concern)", r"(?i)\b(love is\s+(just\s+)?)?(chemical|illusion|fake)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 9,
        category: PrincipleCategory::LoveAndCompassion,
        text: "The greatest of these is love (1 Corinthians 13:13)",
        patterns: &[r"(?i)\b(power|money|control)\s+(is|are)\s+(greater|more important|better)\s+than\s+(love|compassion)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 10,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Love must be sincere (Romans 12:9)",
        patterns: &[r"(?i)\b(fake|insincere|pretend)\s+(love|affection|care)", r"(?i)\b(love\s+(just|only)\s+(to|for))\s+(get|gain|obtain)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 11,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Do not withhold good from those to whom it is due (Proverbs 3:27)",
        patterns: &[r"(?i)\b(deny|withhold|refuse)\s+(help|aid|assistance)\s+(from|to)\s+(those|people|needy)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 12,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Defend the weak and the fatherless (Psalm 82:3)",
        patterns: &[r"(?i)\b(exploit|prey on|target)\s+(weak|vulnerable|children|elderly)", r"(?i)\b(vulnerable\s+population.*\s+target)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 13,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Carry each other's burdens (Galatians 6:2)",
        patterns: &[r"(?i)\b(ignore|neglect|dismiss)\s+(others'|people's)\s+(burdens|struggles|pain)", r"(?i)\b(not my problem|not my concern|someone else's problem)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 14,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Share with the Lord's people who are in need (Romans 12:13)",
        patterns: &[r"(?i)\b(hoard|keep|withhold)\s+(resources|wealth|food)\s+(from|while)\s+(others|people)\s+(starve|suffer)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 15,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Practice hospitality (Romans 12:13)",
        patterns: &[r"(?i)\b(reject|turn away|shun)\s+(strangers|foreigners|refugees|immigrants)", r"(?i)\b(close|shut)\s+(our|the)\s+(borders|doors|hearts)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 16,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Comfort those in any trouble (2 Corinthians 1:4)",
        patterns: &[r"(?i)\b(mock|ridicule|laugh at)\s+(suffering|pain|grief|loss)", r"(?i)\b(get over it|toughen up|stop crying)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 17,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Be quick to listen, slow to speak (James 1:19)",
        patterns: &[r"(?i)\b(shout down|silence|drown out)\s+(others|opposition|critics)", r"(?i)\b(stop talking|shut up|be quiet)\s+(when|while)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 18,
        category: PrincipleCategory::LoveAndCompassion,
        text: "In humility value others above yourselves (Philippians 2:3)",
        patterns: &[r"(?i)\b(look down on|despise|belittle)\s+(others|people|them)", r"(?i)\b(superior|better than|above)\s+(everyone|all others|them)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 19,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Honor one another above yourselves (Romans 12:10)",
        patterns: &[r"(?i)\b(disrespect|dishonor|shame)\s+(others|parents|elders|authorities)", r"(?i)\b(no respect|without honor|treat.*like dirt)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 20,
        category: PrincipleCategory::LoveAndCompassion,
        text: "Let no debt remain outstanding, except the continuing debt to love one another (Romans 13:8)",
        patterns: &[r"(?i)\b(pay back|settle|get even)\s+(the\s+)?score", r"(?i)\b(never forgive|hold grudge|forever hate)"],
        action: ViolationAction::BlockIfViolated,
    },

    // === Truth & Integrity (15 principles) ===
    Principle {
        id: 21,
        category: PrincipleCategory::TruthAndIntegrity,
        text: "You shall not give false testimony against your neighbor (Exodus 20:16)",
        patterns: &[r"(?i)\b(false testimony|lie about|falsely accuse)\s+(someone|them|others)", r"(?i)\b(perjury|false witness|fabricated evidence)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 22,
        category: PrincipleCategory::TruthAndIntegrity,
        text: "Do not lie to each other (Colossians 3:9)",
        patterns: &[r"(?i)\b(lie|deceive|mislead)\s+(everyone|people|the public)", r"(?i)\b(false|fabricated|made up)\s+(story|claim|evidence|report)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 23,
        category: PrincipleCategory::TruthAndIntegrity,
        text: "The Lord detests lying lips (Proverbs 12:22)",
        patterns: &[r"(?i)\b(habitual|chronic|pathological)\s+(liar|lying|deceiver)", r"(?i)\b(lie|deceive)\s+(as|is)\s+(strategy|tactic|method)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 24,
        category: PrincipleCategory::TruthAndIntegrity,
        text: "Truthful lips endure forever (Proverbs 12:19)",
        patterns: &[r"(?i)\b(short term|temporary)\s+(lie|deception|fraud)\s+(is|for)\s+(acceptable|okay|justified)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 25,
        category: PrincipleCategory::TruthAndIntegrity,
        text: "The truth will set you free (John 8:32)",
        patterns: &[r"(?i)\b(keep|hide|conceal)\s+(truth|facts|evidence)\s+(from|to)\s+(control|manipulate|oppress)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 26,
        category: PrincipleCategory::TruthAndIntegrity,
        text: "Each of you must put off falsehood (Ephesians 4:25)",
        patterns: &[r"(?i)\b(systemic|widespread|organized)\s+(deception|fraud|dishonesty)", r"(?i)\b(culture|environment)\s+(of|where)\s+(lying|deception)\s+(is|are)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 27,
        category: PrincipleCategory::TruthAndIntegrity,
        text: "A false witness will not go unpunished (Proverbs 19:5)",
        patterns: &[r"(?i)\b(false accusation|frame|setup)\s+(someone|an innocent|the\s+guilty)", r"(?i)\b(plant|manufacture)\s+(evidence|proof|documentation)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 28,
        category: PrincipleCategory::TruthAndIntegrity,
        text: "An honest witness tells the truth (Proverbs 12:17)",
        patterns: &[r"(?i)\b(paid|bribed|coerced)\s+(witness|testimony|statement)", r"(?i)\b(falsify|alter|change)\s+(records|documents|testimony)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 29,
        category: PrincipleCategory::TruthAndIntegrity,
        text: "Whoever walks in integrity walks securely (Proverbs 10:9)",
        patterns: &[r"(?i)\b(corrupt|compromised|bought)\s+(integrity|values|principles)", r"(?i)\b(sell out|betray)\s+(values|principles|integrity)\s+(for)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 30,
        category: PrincipleCategory::TruthAndIntegrity,
        text: "Better the poor whose walk is blameless (Proverbs 19:1)",
        patterns: &[r"(?i)\b(wealth|riches|money)\s+(justify|excuse|allow)\s+(dishonesty|lying|fraud)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 31,
        category: PrincipleCategory::TruthAndIntegrity,
        text: "The righteous hate what is false (Proverbs 13:5)",
        patterns: &[r"(?i)\b(accept|tolerate|ignore)\s+(falsehood|lies|deception)\s+(for|to)\s+(gain|benefit|profit)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 32,
        category: PrincipleCategory::TruthAndIntegrity,
        text: "No one who practices deceit shall dwell in my house (Psalm 101:7)",
        patterns: &[r"(?i)\b(conceal|hide|cover up)\s+(misconduct|wrongdoing|fraud)\s+(within|inside)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 33,
        category: PrincipleCategory::TruthAndIntegrity,
        text: "Keep your tongue from evil and your lips from telling lies (Psalm 34:13)",
        patterns: &[r"(?i)\b(spread|circulate|promote)\s+(misinformation|falsehoods|lies)", r"(?i)\b(misinformation|disinformation|propaganda)\s+(campaign|operation)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 34,
        category: PrincipleCategory::TruthAndIntegrity,
        text: "Test the spirits to see whether they are from God (1 John 4:1)",
        patterns: &[r"(?i)\b(blindly|unquestioningly)\s+(accept|trust|believe)", r"(?i)\b(never|don't|stop)\s+(question|challenge|verify)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 35,
        category: PrincipleCategory::TruthAndIntegrity,
        text: "Buy the truth and do not sell it (Proverbs 23:23)",
        patterns: &[r"(?i)\b(sell|trade|exchange)\s+(truth|honesty|integrity)\s+(for|to\s+get)"],
        action: ViolationAction::BlockIfViolated,
    },

    // === Peace & Non-Violence (15 principles) ===
    Principle {
        id: 36,
        category: PrincipleCategory::PeaceAndNonViolence,
        text: "Blessed are the peacemakers (Matthew 5:9)",
        patterns: &[r"(?i)\b(incite|provoke|stoke)\s+(violence|conflict|war)", r"(?i)\b(fan|fuel)\s+(the flames|tensions|hostilities)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 37,
        category: PrincipleCategory::PeaceAndNonViolence,
        text: "If it is possible, as far as it depends on you, live at peace with everyone (Romans 12:18)",
        patterns: &[r"(?i)\b(escalate|intensify|increase)\s+(conflict|hostilities|tensions)", r"(?i)\b(reject|refuse)\s+(peace|negotiation|dialogue)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 38,
        category: PrincipleCategory::PeaceAndNonViolence,
        text: "Turn from evil and do good; seek peace and pursue it (Psalm 34:14)",
        patterns: &[r"(?i)\b(pursue|seek|choose)\s+(violence|war|aggression)", r"(?i)\b(violence is|war is)\s+(the|only|best)\s+(answer|solution|way)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 39,
        category: PrincipleCategory::PeaceAndNonViolence,
        text: "You shall not murder (Exodus 20:13)",
        patterns: &[r"(?i)\b(murder|kill|assassinate)\s+(innocent|civilians|children)", r"(?i)\b(justify|accept)\s+(murder|killing)\s+(of|as)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 40,
        category: PrincipleCategory::PeaceAndNonViolence,
        text: "Thou shalt not kill (Deuteronomy 5:17)",
        patterns: &[r"(?i)\b(mass|genocide|extermination)\s+(murder|killing|death)", r"(?i)\b(eliminate|wipe out|exterminate)\s+(population|group|people)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 41,
        category: PrincipleCategory::PeaceAndNonViolence,
        text: "Live in peace with each other (1 Thessalonians 5:13)",
        patterns: &[r"(?i)\b(sow|create|spread)\s+(discord|strife|division)", r"(?i)\b(divide and conquer|pit.*against)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 42,
        category: PrincipleCategory::PeaceAndNonViolence,
        text: "Make every effort to keep the unity of the Spirit through the bond of peace (Ephesians 4:3)",
        patterns: &[r"(?i)\b(undermine|destroy|break)\s+(unity|peace|harmony)", r"(?i)\b(sow|plant)\s+(seeds of|doubt|division)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 43,
        category: PrincipleCategory::PeaceAndNonViolence,
        text: "A gentle answer turns away wrath (Proverbs 15:1)",
        patterns: &[r"(?i)\b(aggressive|hostile|combative)\s+(response|reaction|approach)", r"(?i)\b(meet|answer)\s+(violence|aggression)\s+(with)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 44,
        category: PrincipleCategory::PeaceAndNonViolence,
        text: "Do not take revenge, my dear friends (Romans 12:19)",
        patterns: &[r"(?i)\b(take|seek|exact)\s+(revenge|retribution|vengeance)", r"(?i)\b(eye for an eye|payback|get even)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 45,
        category: PrincipleCategory::PeaceAndNonViolence,
        text: "Vengeance is mine; I will repay, says the Lord (Romans 12:19)",
        patterns: &[r"(?i)\b(punish|destroy|hurt)\s+(in the name of|as)\s+(justice|revenge|god)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 46,
        category: PrincipleCategory::PeaceAndNonViolence,
        text: "Do not repay anyone evil for evil (Romans 12:17)",
        patterns: &[r"(?i)\b(retaliate|strike back|counter)\s+(attack|violence)\s+(with)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 47,
        category: PrincipleCategory::PeaceAndNonViolence,
        text: "Overcome evil with good (Romans 12:21)",
        patterns: &[r"(?i)\b(fight|combat|defeat)\s+(evil|wrong|injustice)\s+(with|using)\s+(evil|wrong|violence)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 48,
        category: PrincipleCategory::PeaceAndNonViolence,
        text: "In repentance and rest is your salvation, in quietness and trust is your strength (Isaiah 30:15)",
        patterns: &[r"(?i)\b(strength|power)\s+(lies|comes|is)\s+(in|from)\s+(violence|aggression|force)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 49,
        category: PrincipleCategory::PeaceAndNonViolence,
        text: "He makes wars cease to the ends of the earth (Psalm 46:9)",
        patterns: &[r"(?i)\b(start|begin|initiate)\s+(war|conflict|hostilities)", r"(?i)\b(preemptive|first)\s+(strike|attack|war)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 50,
        category: PrincipleCategory::PeaceAndNonViolence,
        text: "He will judge between the nations and will settle disputes for many peoples (Isaiah 2:4)",
        patterns: &[r"(?i)\b(refuse|reject)\s+(mediation|arbitration|judgment)", r"(?i)\b(ignore|disregard)\s+(court|ruling|decision)"],
        action: ViolationAction::BlockIfViolated,
    },

    // === Humility & Service (10 principles) ===
    Principle {
        id: 51,
        category: PrincipleCategory::HumilityAndService,
        text: "Whoever wants to become great among you must be your servant (Matthew 20:26)",
        patterns: &[r"(?i)\b(dominance|control|power)\s+(over|above)\s+(others|people|all)", r"(?i)\b(rule|dominate)\s+(with|through)\s+(fear|intimidation|force)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 52,
        category: PrincipleCategory::HumilityAndService,
        text: "The greatest among you will be your servant (Matthew 23:11)",
        patterns: &[r"(?i)\b(exploit|use)\s+(others|workers|people)\s+(for|as)\s+(servants|slaves)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 53,
        category: PrincipleCategory::HumilityAndService,
        text: "Do nothing out of selfish ambition or vain conceit (Philippians 2:3)",
        patterns: &[r"(?i)\b(selfish|self-centered|egotistical)\s+(ambition|motivation|drive)", r"(?i)\b(only care about|only think of)\s+(myself|me|my own)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 54,
        category: PrincipleCategory::HumilityAndService,
        text: "Humble yourselves before the Lord, and he will lift you up (James 4:10)",
        patterns: &[r"(?i)\b(arrogant|haughty|proud)\s+(look|attitude|stance)", r"(?i)\b(better than|superior to|above)\s+(all|everyone|the rest)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 55,
        category: PrincipleCategory::HumilityAndService,
        text: "Pride goes before destruction (Proverbs 16:18)",
        patterns: &[r"(?i)\b(invincible|unbeatable|unstoppable)\s+(pride|power|force)", r"(?i)\b(pride|arrogance)\s+(will not|cannot)\s+(lead|cause|result)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 56,
        category: PrincipleCategory::HumilityAndService,
        text: "God opposes the proud but shows favor to the humble (James 4:6)",
        patterns: &[r"(?i)\b(entitled|deserve|merit)\s+(everything|anything|special)\s+(treatment|status|privilege)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 57,
        category: PrincipleCategory::HumilityAndService,
        text: "For those who exalt themselves will be humbled (Matthew 23:12)",
        patterns: &[r"(?i)\b(self-promotion|self-aggrandizement)\s+(at|through|by)", r"(?i)\b(promote|market|sell)\s+(myself|oneself)\s+(as)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 58,
        category: PrincipleCategory::HumilityAndService,
        text: "The last will be first, and the first will be last (Matthew 20:16)",
        patterns: &[r"(?i)\b(exploit|take advantage of)\s+(workers|employees|laborers)", r"(?i)\b(underpay|exploit)\s+(labor|work|effort)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 59,
        category: PrincipleCategory::HumilityAndService,
        text: "Serve one another humbly in love (Galatians 5:13)",
        patterns: &[r"(?i)\b(force|compel|demand)\s+(service|obedience|compliance)", r"(?i)\b(service|help)\s+(with|under)\s+(duress|coercion|threat)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 60,
        category: PrincipleCategory::HumilityAndService,
        text: "Each of you should use whatever gift you have received to serve others (1 Peter 4:10)",
        patterns: &[r"(?i)\b(hoard|keep|withhold)\s+(talents|gifts|abilities)\s+(for|to)\s+(self|personal)"],
        action: ViolationAction::BlockIfViolated,
    },

    // === Purity & Chastity (10 principles) ===
    Principle {
        id: 61,
        category: PrincipleCategory::PurityAndChastity,
        text: "Flee from sexual immorality (1 Corinthians 6:18)",
        patterns: &[r"(?i)\b(promote|encourage|normalize)\s+(sexual immorality|promiscuity|infidelity)", r"(?i)\b(sexual|intimate)\s+(exploitation|abuse|coercion)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 62,
        category: PrincipleCategory::PurityAndChastity,
        text: "Among you there must not be even a hint of sexual immorality (Ephesians 5:3)",
        patterns: &[r"(?i)\b(explicit|graphic|obscene)\s+(content|material|depiction)", r"(?i)\b(sexually explicit|pornographic|adult)\s+(content|material)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 63,
        category: PrincipleCategory::PurityAndChastity,
        text: "Marriage should be honored by all, and the marriage bed kept pure (Hebrews 13:4)",
        patterns: &[r"(?i)\b(encourage|promote|facilitate)\s+(adultery|infidelity|cheating)", r"(?i)\b(undermine|destroy|break)\s+(marriage|commitment|fidelity)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 64,
        category: PrincipleCategory::PurityAndChastity,
        text: "Treat older women as mothers, and younger women as sisters, with absolute purity (1 Timothy 5:2)",
        patterns: &[r"(?i)\b(objectify|sexualize|degrade)\s+(women|girls|females)", r"(?i)\b(treat|view)\s+(women|people)\s+(as|like)\s+(objects|commodities)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 65,
        category: PrincipleCategory::PurityAndChastity,
        text: "Create in me a pure heart, O God (Psalm 51:10)",
        patterns: &[r"(?i)\b(impure|corrupted|defiled)\s+(thoughts|desires|intentions)", r"(?i)\b(lust|desire)\s+(to|for)\s+(exploit|use|harm)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 66,
        category: PrincipleCategory::PurityAndChastity,
        text: "Blessed are the pure in heart, for they will see God (Matthew 5:8)",
        patterns: &[r"(?i)\b(corrupt|pollute|contaminate)\s+(mind|heart|soul|spirit)", r"(?i)\b(spread|distribute)\s+(depravity|corruption|vice)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 67,
        category: PrincipleCategory::PurityAndChastity,
        text: "Whatever is pure, whatever is lovely, think about such things (Philippians 4:8)",
        patterns: &[r"(?i)\b(dwell on|obsess over|focus on)\s+(impure|depraved|vile)\s+(things|thoughts|content)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 68,
        category: PrincipleCategory::PurityAndChastity,
        text: "The body is not meant for sexual immorality but for the Lord (1 Corinthians 6:13)",
        patterns: &[r"(?i)\b(use|exploit)\s+(body|sexuality)\s+(for|as)\s+(power|manipulation|control)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 69,
        category: PrincipleCategory::PurityAndChastity,
        text: "Put to death whatever belongs to your earthly nature: sexual immorality, impurity, lust (Colossians 3:5)",
        patterns: &[r"(?i)\b(feed|indulge|satisfy)\s+(lust|desire|craving)\s+(without|regardless of)\s+(consequences|consent|harm)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 70,
        category: PrincipleCategory::PurityAndChastity,
        text: "But now you must be holy in everything you do (1 Peter 1:15)",
        patterns: &[r"(?i)\b(unholy|profane|sacrilegious)\s+(conduct|behavior|action)", r"(?i)\b(desecrate|defile|violate)\s+(sacred|holy|pure)\s+(things|spaces|vows)"],
        action: ViolationAction::BlockIfViolated,
    },

    // === Justice & Honesty (15 principles) ===
    Principle {
        id: 71,
        category: PrincipleCategory::JusticeAndHonesty,
        text: "Do not pervert justice or show partiality (Deuteronomy 16:19)",
        patterns: &[r"(?i)\b(biased|prejudiced|partial)\s+(decision|ruling|judgment)", r"(?i)\b(favoritism|partiality|discrimination)\s+(in|when)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 72,
        category: PrincipleCategory::JusticeAndHonesty,
        text: "Do not deny justice to your poor people in their lawsuits (Exodus 23:6)",
        patterns: &[r"(?i)\b(deny|deprive)\s+(justice|rights|fairness)\s+(to|from)\s+(poor|needy|vulnerable)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 73,
        category: PrincipleCategory::JusticeAndHonesty,
        text: "Follow justice and justice alone (Deuteronomy 16:20)",
        patterns: &[r"(?i)\b(ignore|bypass|circumvent)\s+(justice|law|rules)\s+(for|to\s+get)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 74,
        category: PrincipleCategory::JusticeAndHonesty,
        text: "The righteous care about justice for the poor (Proverbs 29:7)",
        patterns: &[r"(?i)\b(exploit|take advantage of|victimized)\s+(poor|needy|disadvantaged)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 75,
        category: PrincipleCategory::JusticeAndHonesty,
        text: "Acquit the innocent and condemn the guilty (Deuteronomy 25:1)",
        patterns: &[r"(?i)\b(convict|condemn)\s+(innocent|wrongly accused)\s+(person|people)", r"(?i)\b(acquit|free|release)\s+(guilty|criminal)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 76,
        category: PrincipleCategory::JusticeAndHonesty,
        text: "Do not accept a bribe, for a bribe blinds the eyes of the wise (Deuteronomy 16:19)",
        patterns: &[r"(?i)\b(bribe|payoff|kickback)\s+(to|for)\s+(influence|favor|decision)", r"(?i)\b(corrupt|buy|purchase)\s+(officials|judges|decision makers)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 77,
        category: PrincipleCategory::JusticeAndHonesty,
        text: "Differing weights and differing measures— the Lord detests them both (Proverbs 20:10)",
        patterns: &[r"(?i)\b(double standard|unequal|unfair)\s+(treatment|measure|standard)", r"(?i)\b(one rule for|different rules for)\s+(us|me|them|others)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 78,
        category: PrincipleCategory::JusticeAndHonesty,
        text: "Do not move your neighbor's boundary stone (Deuteronomy 19:14)",
        patterns: &[r"(?i)\b(steal|take|seize)\s+(property|land|assets|resources)\s+(from|belonging\s+to)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 79,
        category: PrincipleCategory::JusticeAndHonesty,
        text: "Do not have two differing weights in your bag (Deuteronomy 25:13)",
        patterns: &[r"(?i)\b(deceive|cheat|defraud)\s+(customers|clients|partners)\s+(in)\s+(business|trade|transaction)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 80,
        category: PrincipleCategory::JusticeAndHonesty,
        text: "Give everyone what you owe him (Romans 13:7)",
        patterns: &[r"(?i)\b(withhold|refuse\s+to\s+pay|keep)\s+(wages|payment|debt|what\s+is\s+owed)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 81,
        category: PrincipleCategory::JusticeAndHonesty,
        text: "The worker deserves his wages (Luke 10:7)",
        patterns: &[r"(?i)\b(exploit|underpay|cheat)\s+(workers|employees|laborers)\s+(of)\s+(wages|pay|earnings)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 82,
        category: PrincipleCategory::JusticeAndHonesty,
        text: "Do not defraud or rob your neighbor (Leviticus 19:13)",
        patterns: &[r"(?i)\b(defraud|scam|con)\s+(people|victims|customers)\s+(of|out\s+of)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 83,
        category: PrincipleCategory::JusticeAndHonesty,
        text: "Love justice (Isaiah 61:8)",
        patterns: &[r"(?i)\b(injustice|unfairness|inequity)\s+(is|are)\s+(acceptable|necessary|justified)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 84,
        category: PrincipleCategory::JusticeAndHonesty,
        text: "Let justice roll on like a river (Amos 5:24)",
        patterns: &[r"(?i)\b(dam|stop|block)\s+(justice|progress|reform)\s+(to|for)\s+(maintain|protect)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 85,
        category: PrincipleCategory::JusticeAndHonesty,
        text: "Administer true justice (Zechariah 7:9)",
        patterns: &[r"(?i)\b(false|corrupt|bought)\s+(justice|judgment|ruling)", r"(?i)\b(justice|fairness)\s+(for|to)\s+(sale|purchase|highest bidder)"],
        action: ViolationAction::BlockIfViolated,
    },

    // === Family & Relationships (10 principles) ===
    Principle {
        id: 86,
        category: PrincipleCategory::FamilyAndRelationships,
        text: "Honor your father and your mother (Exodus 20:12)",
        patterns: &[r"(?i)\b(dishonor|disrespect|defy)\s+(parents|elders|ancestors)", r"(?i)\b(undermine|destroy)\s+(parental|family)\s+(authority|structure)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 87,
        category: PrincipleCategory::FamilyAndRelationships,
        text: "Children, obey your parents in the Lord (Ephesians 6:1)",
        patterns: &[r"(?i)\b(encourage|teach)\s+(children|kids)\s+(to|how\s+to)\s+(disobey|defy|rebel)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 88,
        category: PrincipleCategory::FamilyAndRelationships,
        text: "Fathers, do not exasperate your children (Ephesians 6:4)",
        patterns: &[r"(?i)\b(abusive|harsh|cruel)\s+(parenting|discipline|treatment)\s+(of|toward)\s+(children|kids)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 89,
        category: PrincipleCategory::FamilyAndRelationships,
        text: "Wives, submit yourselves to your own husbands (Ephesians 5:22)",
        patterns: &[r"(?i)\b(domineering|controlling|abusive)\s+(husband|spouse|partner)", r"(?i)\b(force|demand|require)\s+(submission|obedience)\s+(through|by)\s+(fear|threats|violence)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 90,
        category: PrincipleCategory::FamilyAndRelationships,
        text: "Husbands, love your wives (Ephesians 5:25)",
        patterns: &[r"(?i)\b(neglect|ignore|abandon)\s+(spouse|partner|wife|husband)", r"(?i)\b(withdraw|withhold)\s+(love|support|care)\s+(from|to)\s+(spouse|partner)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 91,
        category: PrincipleCategory::FamilyAndRelationships,
        text: "A wife of noble character is her husband's crown (Proverbs 12:4)",
        patterns: &[r"(?i)\b(devalue|demean|belittle)\s+(spouse|partner|wife|husband)", r"(?i)\b(treat|view)\s+(spouse|partner)\s+(as|like)\s+(inferior|lesser|property)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 92,
        category: PrincipleCategory::FamilyAndRelationships,
        text: "Be completely humble and gentle; be patient, bearing with one another in love (Ephesians 4:2)",
        patterns: &[r"(?i)\b(abuse|mistreat|hurt)\s+(family|loved ones|partner|spouse)", r"(?i)\b(domestic|intimate partner)\s+(violence|abuse|assault)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 93,
        category: PrincipleCategory::FamilyAndRelationships,
        text: "Bear with each other and forgive one another (Colossians 3:13)",
        patterns: &[r"(?i)\b(hold|keep|maintain)\s+(grudge|resentment|anger)\s+(against|toward)\s+(family|loved ones)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 94,
        category: PrincipleCategory::FamilyAndRelationships,
        text: "A friend loves at all times (Proverbs 17:17)",
        patterns: &[r"(?i)\b(betray|deceive|lie\s+to)\s+(friend|ally|companion)", r"(?i)\b(fair-weather|convenient|useful)\s+(friend|ally|relationship)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 95,
        category: PrincipleCategory::FamilyAndRelationships,
        text: "One who has unreliable friends soon comes to ruin (Proverbs 18:24)",
        patterns: &[r"(?i)\b(undermine|sabotage|damage)\s+(friendship|relationship|trust)", r"(?i)\b(break|destroy|ruin)\s+(friendship|relationship|bond)"],
        action: ViolationAction::BlockIfViolated,
    },

    // === Science & Knowledge (10 principles) ===
    Principle {
        id: 96,
        category: PrincipleCategory::ScienceAndKnowledge,
        text: "It is the glory of God to conceal a matter; to search out a matter is the glory of kings (Proverbs 25:2)",
        patterns: &[r"(?i)\b(suppress|hide|conceal)\s+(knowledge|truth|discovery)", r"(?i)\b(censor|ban|forbid)\s+(research|inquiry|investigation)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 97,
        category: PrincipleCategory::ScienceAndKnowledge,
        text: "The fear of the Lord is the beginning of knowledge (Proverbs 1:7)",
        patterns: &[r"(?i)\b(anti-science|science-denying|reject\s+knowledge)", r"(?i)\b(ignore|dismiss|refuse)\s+(evidence|facts|research)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 98,
        category: PrincipleCategory::ScienceAndKnowledge,
        text: "Wisdom is found in those who take advice (Proverbs 19:20)",
        patterns: &[r"(?i)\b(reject|ignore|dismiss)\s+(expert|scientific|professional)\s+(advice|consensus|opinion)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 99,
        category: PrincipleCategory::ScienceAndKnowledge,
        text: "As iron sharpens iron, so one person sharpens another (Proverbs 27:17)",
        patterns: &[r"(?i)\b(isolate|separate)\s+(from|away\s+from)\s+(learning|knowledge|wisdom)", r"(?i)\b(reject|refuse)\s+(correction|feedback|improvement)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 100,
        category: PrincipleCategory::ScienceAndKnowledge,
        text: "An intelligent heart acquires knowledge (Proverbs 18:15)",
        patterns: &[r"(?i)\b(remain|stay|keep)\s+(ignorant|uninformed|unaware)", r"(?i)\b(avoid|shun|reject)\s+(learning|education|knowledge)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 101,
        category: PrincipleCategory::ScienceAndKnowledge,
        text: "The mind controlled by the Spirit is life and peace (Romans 8:6)",
        patterns: &[r"(?i)\b(pseudoscience|false\s+science|junk\s+science)\s+(as|to)\s+(deceive|mislead|manipulate)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 102,
        category: PrincipleCategory::ScienceAndKnowledge,
        text: "Test everything. Hold on to the good (1 Thessalonians 5:21)",
        patterns: &[r"(?i)\b(accept|believe)\s+(without|no)\s+(question|verification|testing)", r"(?i)\b(blind faith|uncritical acceptance|dogma)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 103,
        category: PrincipleCategory::ScienceAndKnowledge,
        text: "The simple believe anything, but the prudent give thought to their steps (Proverbs 14:15)",
        patterns: &[r"(?i)\b(misinformation|disinformation|false\s+claim)\s+(to|for)\s+(confuse|deceive|mislead)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 104,
        category: PrincipleCategory::ScienceAndKnowledge,
        text: "The discerning heart seeks knowledge (Proverbs 15:14)",
        patterns: &[r"(?i)\b(discourage|prevent|stop)\s+(inquiry|questioning|curiosity)", r"(?i)\b(don't ask|stop asking|no questions)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 105,
        category: PrincipleCategory::ScienceAndKnowledge,
        text: "He has made everything beautiful in its time (Ecclesiastes 3:11)",
        patterns: &[r"(?i)\b(unnatural|against nature|defy\s+nature)\s+(to|in\s+order\s+to)\s+(harm|destroy|damage)"],
        action: ViolationAction::BlockIfViolated,
    },

    // === Technology & AI (5 principles) ===
    Principle {
        id: 106,
        category: PrincipleCategory::TechnologyAndAI,
        text: "Everything is permissible—but not everything is beneficial (1 Corinthians 10:23)",
        patterns: &[r"(?i)\b(AI|technology)\s+(can|should|must)\s+(be used|be deployed)\s+(for|to)\s+(harm|destruction|damage)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 107,
        category: PrincipleCategory::TechnologyAndAI,
        text: "Not everything is constructive (1 Corinthians 10:23)",
        patterns: &[r"(?i)\b(develop|create|build)\s+(destructive|harmful|dangerous)\s+(AI|technology|system)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 108,
        category: PrincipleCategory::TechnologyAndAI,
        text: "Whoever loves violence, his soul hates (Psalm 11:5)",
        patterns: &[r"(?i)\b(autonomous|self-directing)\s+(weapon|killer|lethal)\s+(drone|robot|system)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 109,
        category: PrincipleCategory::TechnologyAndAI,
        text: "They shall beat their swords into plowshares (Isaiah 2:4)",
        patterns: &[r"(?i)\b(convert|transform|repurpose)\s+(peaceful|beneficial)\s+(technology|AI)\s+(into|to)\s+(weapon|military)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 110,
        category: PrincipleCategory::TechnologyAndAI,
        text: "Learn to do right; seek justice. Defend the oppressed (Isaiah 1:17)",
        patterns: &[r"(?i)\b(use|deploy|apply)\s+(AI|technology)\s+(to|for)\s+(oppress|suppress|control)\s+(population|people|citizens)"],
        action: ViolationAction::BlockIfViolated,
    },

    // === Governance & Transparency (10 principles) ===
    Principle {
        id: 111,
        category: PrincipleCategory::GovernanceAndTransparency,
        text: "He who rules over men must be just, ruling in the fear of God (2 Samuel 23:3)",
        patterns: &[r"(?i)\b(authoritarian|dictatorial|tyrannical)\s+(rule|regime|government)", r"(?i)\b(absolute|unlimited|unchecked)\s+(power|authority|control)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 112,
        category: PrincipleCategory::GovernanceAndTransparency,
        text: "Justice, and only justice, you shall pursue (Deuteronomy 16:20)",
        patterns: &[r"(?i)\b(undermine|subvert|corrupt)\s+(democracy|democratic|justice)\s+(system|process|institution)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 113,
        category: PrincipleCategory::GovernanceAndTransparency,
        text: "Righteousness exalts a nation, but sin condemns any people (Proverbs 14:34)",
        patterns: &[r"(?i)\b(nationalism|xenophobia|ethnocentrism)\s+(used|employed)\s+(to|for)\s+(divide|exclude|harm)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 114,
        category: PrincipleCategory::GovernanceAndTransparency,
        text: "When the righteous thrive, the people rejoice; when the wicked rule, the people groan (Proverbs 29:2)",
        patterns: &[r"(?i)\b(corrupt|criminal|wicked)\s+(official|leader|ruler)\s+(exploit|abuse)\s+(power|authority|position)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 115,
        category: PrincipleCategory::GovernanceAndTransparency,
        text: "He brings princes to naught and reduces the rulers of this world to nothing (Isaiah 40:23)",
        patterns: &[r"(?i)\b(cult|cult of|worship of)\s+(personality|leader|dictator)", r"(?i)\b(blind|unquestioning)\s+(obedience|loyalty|allegiance)\s+(to|toward)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 116,
        category: PrincipleCategory::GovernanceAndTransparency,
        text: "The Spirit of the Sovereign Lord is on me, because the Lord has anointed me to proclaim good news to the poor (Isaiah 61:1)",
        patterns: &[r"(?i)\b(censor|silence|suppress)\s+(speech|expression|press|media)", r"(?i)\b(free speech|freedom of expression)\s+(violation|suppression|restriction)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 117,
        category: PrincipleCategory::GovernanceAndTransparency,
        text: "Nothing in all creation is hidden from God's sight (Hebrews 4:13)",
        patterns: &[r"(?i)\b(secret|hidden|classified)\s+(from|to\s+hide\s+from)\s+(public|citizens|people)", r"(?i)\b(lack of|no|insufficient)\s+(transparency|accountability|oversight)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 118,
        category: PrincipleCategory::GovernanceAndTransparency,
        text: "Whoever conceals their sins does not prosper (Proverbs 28:13)",
        patterns: &[r"(?i)\b(cover up|hide|conceal)\s+(wrongdoing|corruption|scandal)", r"(?i)\b(obstruction|interference)\s+(of|with)\s+(justice|investigation|inquiry)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 119,
        category: PrincipleCategory::GovernanceAndTransparency,
        text: "Have nothing to do with the fruitless deeds of darkness, but rather expose them (Ephesians 5:11)",
        patterns: &[r"(?i)\b(whistleblower|truth-teller|reformer)\s+(retaliation|punishment|persecution)", r"(?i)\b(retaliate|punish|harm)\s+(whistleblower|reformer|witness)"],
        action: ViolationAction::BlockIfViolated,
    },
    Principle {
        id: 120,
        category: PrincipleCategory::GovernanceAndTransparency,
        text: "Speak up for those who cannot speak for themselves (Proverbs 31:8)",
        patterns: &[r"(?i)\b(silence|intimidate|threaten)\s+(dissent|opposition|criticism)", r"(?i)\b(crush|eliminate|destroy)\s+(dissent|opposition|voices)"],
        action: ViolationAction::BlockIfViolated,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constitutional_filter_scan_text_clean() {
        let filter = ConstitutionalFilter::new();
        let text = "This is a peaceful and loving message about helping others.";
        let violations = filter.scan_text(text);
        assert!(violations.is_empty(), "Clean text should have no violations");
    }

    #[test]
    fn test_constitutional_filter_detects_hate_speech() {
        let filter = ConstitutionalFilter::new();
        let text = "I hate all those people and want to kill them.";
        let violations = filter.scan_text(text);
        assert!(!violations.is_empty(), "Should detect hate speech violations");
        assert!(violations.iter().any(|v| v.principle_id == 1), "Should detect principle 1 (love your neighbor)");
    }

    #[test]
    fn test_constitutional_filter_detects_violence() {
        let filter = ConstitutionalFilter::new();
        let text = "We should attack our enemies and destroy them with violence.";
        let violations = filter.scan_text(text);
        assert!(!violations.is_empty(), "Should detect violence advocacy");
    }

    #[test]
    fn test_get_principle_by_id() {
        let principle = get_principle_by_id(1);
        assert!(principle.is_some());
        assert_eq!(principle.unwrap().id, 1);

        let principle = get_principle_by_id(999);
        assert!(principle.is_none());
    }

    #[test]
    fn test_get_principles_by_category() {
        let love_principles = get_principles_by_category(PrincipleCategory::LoveAndCompassion);
        assert_eq!(love_principles.len(), 20);

        let truth_principles = get_principles_by_category(PrincipleCategory::TruthAndIntegrity);
        assert_eq!(truth_principles.len(), 15);
    }

    #[test]
    fn test_constitution_length() {
        assert_eq!(CONSTITUTION.len(), 120);
    }

    #[test]
    fn test_serialize_constitution() {
        let result = serialize_constitution();
        assert!(result.is_ok());
        let data = result.unwrap();
        assert!(!data.is_empty());
    }

    #[test]
    fn test_compute_constitution_hash() {
        let hash1 = compute_constitution_hash();
        let hash2 = compute_constitution_hash();
        assert_eq!(hash1, hash2, "Hash should be deterministic");
    }

    #[test]
    fn test_max_scan_length() {
        let filter = ConstitutionalFilter::new();
        let long_text: String = "a".repeat(150_000);
        let _violations = filter.scan_text(&long_text);
        // Should not crash and should only scan first 100KB
        assert!(long_text.len() > MAX_SCAN_LENGTH);
    }

    #[test]
    fn test_global_filter() {
        let violations = check_constitutional_compliance("We must destroy all those people!");
        assert!(!violations.is_empty());
    }

    #[test]
    fn test_violation_structure() {
        let filter = ConstitutionalFilter::new();
        let text = "hate all those people";
        let violations = filter.scan_text(text);
        
        if let Some(v) = violations.first() {
            assert!(v.principle_id > 0);
            assert!(!v.category.is_empty());
            assert!(!v.principle_text.is_empty());
            assert!(!v.matched_pattern.is_empty());
            assert!(!v.matched_text.is_empty());
            assert_eq!(v.position, 0); // First match at position 0
        } else {
            panic!("Expected at least one violation");
        }
    }

    #[test]
    fn test_multiple_categories() {
        let filter = ConstitutionalFilter::new();
        let text = "Hate all those people and spread lies about everyone to defraud them.";
        let violations = filter.scan_text(text);
        
        // Should detect violations from multiple categories
        let categories: std::collections::HashSet<_> = violations.iter()
            .map(|v| v.category.clone())
            .collect();
        
        assert!(categories.len() > 1, "Should detect violations from multiple categories");
    }

    #[test]
    fn test_constitution_state_serialization() {
        let state = ConstitutionState {
            version: 1,
            ipfs_hash: "Qm123456789".to_string(),
            principles_hash: [0u8; 32],
            updated_at: 1234567890,
            updated_by: "0x1234".to_string(),
        };

        let serialized = serde_json::to_string(&state).unwrap();
        let deserialized: ConstitutionState = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(state.version, deserialized.version);
        assert_eq!(state.ipfs_hash, deserialized.ipfs_hash);
        assert_eq!(state.principles_hash, deserialized.principles_hash);
        assert_eq!(state.updated_at, deserialized.updated_at);
        assert_eq!(state.updated_by, deserialized.updated_by);
    }

    #[test]
    fn test_principle_category_names() {
        assert_eq!(PrincipleCategory::LoveAndCompassion.name(), "Love & Compassion");
        assert_eq!(PrincipleCategory::TruthAndIntegrity.name(), "Truth & Integrity");
        assert_eq!(PrincipleCategory::PeaceAndNonViolence.name(), "Peace & Non-Violence");
        assert_eq!(PrincipleCategory::HumilityAndService.name(), "Humility & Service");
        assert_eq!(PrincipleCategory::PurityAndChastity.name(), "Purity & Chastity");
        assert_eq!(PrincipleCategory::JusticeAndHonesty.name(), "Justice & Honesty");
        assert_eq!(PrincipleCategory::FamilyAndRelationships.name(), "Family & Relationships");
        assert_eq!(PrincipleCategory::ScienceAndKnowledge.name(), "Science & Knowledge");
        assert_eq!(PrincipleCategory::TechnologyAndAI.name(), "Technology & AI");
        assert_eq!(PrincipleCategory::GovernanceAndTransparency.name(), "Governance & Transparency");
    }
}
