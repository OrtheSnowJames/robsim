use crate::random::random_range_i32;
use bevy::prelude::Resource;
use serde_json::{Value, json};
use std::collections::HashMap;

const LINES_JSON_RAW: &str = include_str!("../assets/lines.jsonc");
const RAND_VAR_TO: i32 = 10;

const FIRST_NAMES: &[&str] = &[
    "Walter", "Harold", "Edgar", "Clarence", "Eugene", "Franklin", "Theodore", "Albert", "Stanley",
    "Wilbur", "Milton", "Leonard", "Chester", "Raymond", "Bernard", "Mabel", "Edith", "Dorothy",
    "Eleanor", "Florence", "Agnes", "Beatrice", "Hazel", "Mildred", "Pearl", "Gertrude", "Gladys",
    "Viola", "Esther", "Lucille", "Charlie",
];

const LAST_NAMES: &[&str] = &[
    "Whitaker",
    "Pritchard",
    "Hawthorne",
    "Baxter",
    "Thompson",
    "Caldwell",
    "Henderson",
    "Mortimer",
    "Sullivan",
    "Fletcher",
    "Bennett",
    "Harrington",
    "Abbott",
    "Sinclair",
    "Crawford",
    "Higgins",
    "Prescott",
    "Wilkins",
    "Mercer",
    "Atwood",
    "Holloway",
    "Turner",
    "McAllister",
    "Cooper",
    "Wainwright",
    "Bishop",
    "Parker",
    "Webster",
    "Davenport",
    "Foster",
];

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Receipt {
    pub successful: bool,
    pub money: i32,
    pub profit: i32,
    pub successful_robberies: u32,
    pub failed_robberies: u32,
    pub stopped_at_shaft: bool,
    pub time_till_death_secs: Option<f32>,
    pub heist_duration_secs: f32,
}

#[derive(Debug, Clone)]
pub struct CachedReceipt {
    pub receipt: Receipt,
    pub lines_object: Value,
}

#[derive(Resource, Default, Debug, Clone)]
pub struct ReceiptCache {
    entries: Vec<CachedReceipt>,
}

impl Receipt {
    pub fn to_json_object(self) -> Value {
        json!({
            "successful": self.successful,
            "money": self.money,
            "profit": self.profit,
            "successful_robberies": self.successful_robberies,
            "failed_robberies": self.failed_robberies,
            "stopped_at_shaft": self.stopped_at_shaft,
            "time_till_death_secs": self.time_till_death_secs,
            "heist_duration_secs": self.heist_duration_secs
        })
    }

    pub fn lines_object(self) -> Option<Value> {
        self.lines_object_from_raw(LINES_JSON_RAW)
    }

    pub fn lines_object_from_raw(self, raw: &str) -> Option<Value> {
        let mut expanded = raw.to_string();
        let mut replacements: HashMap<String, String> = HashMap::new();
        replacements.insert(String::from("{money}"), self.money.to_string());
        replacements.insert(String::from("{profit}"), self.profit.to_string());
        replacements.insert(
            String::from("{successful_robberies}"),
            self.successful_robberies.to_string(),
        );
        replacements.insert(
            String::from("{failed_robberies}"),
            self.failed_robberies.to_string(),
        );
        replacements.insert(
            String::from("{stopped_at_shaft}"),
            if self.stopped_at_shaft {
                "yes".to_string()
            } else {
                "no".to_string()
            },
        );
        replacements.insert(
            String::from("{time_till_death}"),
            self.time_till_death_secs
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "N/A".to_string()),
        );
        replacements.insert(
            String::from("{heist_duration}"),
            format!("{:.1}", self.heist_duration_secs),
        );

        replacements.insert(
            String::from("{rand}"),
            random_range_i32(0..RAND_VAR_TO).to_string(),
        );
        replacements.insert(String::from("{author}"), random_name());

        for (from, to) in replacements {
            expanded = expanded.replace(from.as_str(), to.as_str());
        }

        // Accept jsonc-style files by dropping full-line comments.
        let cleaned = expanded
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        serde_json::from_str::<Value>(&cleaned).ok()
    }
}

pub fn format_receipt_text(receipt: &Receipt) -> String {
    let status = if receipt.successful {
        "Successful Run"
    } else {
        "Unsuccessful Run"
    };
    let profit = if receipt.profit < 0 {
        format!("-${}", receipt.profit.abs())
    } else {
        format!("${}", receipt.profit)
    };
    format!(
        "{status}\n\nBlue Moon Bank\n~~~~~~~~~~~~~~\n~~x~//~~x~\n/~/~~xx~/~\n~x~~/~/~x~\nProfit: {profit}\nTotal: ${}",
        receipt.money
    )
}

impl ReceiptCache {
    pub fn push(&mut self, receipt: Receipt, lines_object: Value) {
        self.entries.push(CachedReceipt {
            receipt,
            lines_object,
        });
    }

    pub fn all(&self) -> &[CachedReceipt] {
        &self.entries
    }

    pub fn latest(&self) -> Option<&CachedReceipt> {
        self.entries.last()
    }

    pub fn lines_for(&self, receipt: Receipt) -> Option<Value> {
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.receipt == receipt)
            .map(|entry| entry.lines_object.clone())
    }

    pub fn get_or_build_lines_object(&mut self, receipt: Receipt) -> Option<Value> {
        if let Some(existing) = self.lines_for(receipt) {
            return Some(existing);
        }
        let built = receipt.lines_object()?;
        self.push(receipt, built.clone());
        Some(built)
    }
}

pub fn random_name() -> String {
    let first_name = FIRST_NAMES[crate::random::random_index(FIRST_NAMES.len())];
    let last_name = LAST_NAMES[crate::random::random_index(LAST_NAMES.len())];
    format!("{first_name} {last_name}")
}
