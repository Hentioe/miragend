use anyhow::Context;
use html5ever::tendril::{fmt::UTF8, Tendril};
use log::{info, warn};
use rand::Rng;
use serde_json::Value;

#[derive(Debug)]
pub struct ObfuscatorConfig {
    pub mappers: Vec<CharactersMapper>,
}

#[derive(Debug, serde::Deserialize)]
pub struct CharactersMapper {
    pub source_start: char,
    pub source_end: char,
    pub target_start: char,
    pub target_end: char,
    pub comment: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct Record {
    pub source_start: String,
    pub source_end: String,
    pub target_start: String,
    pub target_end: String,
    pub comment: String,
}

impl TryFrom<Record> for CharactersMapper {
    type Error = anyhow::Error;

    fn try_from(record: Record) -> Result<Self, Self::Error> {
        let conver_field_to_char = |field: &str, value: &str| {
            char::from_u32(u32::from_str_radix(value, 16).context(format!(
                "failed to parse `{}` as u32 from hex string",
                field
            ))?)
            .context("failed to convert u32 to char")
        };

        Ok(Self {
            source_start: conver_field_to_char("source_start", &record.source_start)?,
            source_end: conver_field_to_char("source_end", &record.source_end)?,
            target_start: conver_field_to_char("target_start", &record.target_start)?,
            target_end: conver_field_to_char("target_end", &record.target_end)?,
            comment: record.comment,
        })
    }
}

impl ObfuscatorConfig {
    pub fn load_from_csv(content: &str) -> Self {
        let mut records = vec![];
        let mut rdr = csv::Reader::from_reader(content.as_bytes());
        for result in rdr.deserialize::<Record>() {
            match result {
                Err(e) => {
                    warn!("failed to parse csv record: {}, ignored", e);
                }
                Ok(record) => {
                    records.push(record);
                }
            };
        }

        let mut mappers = vec![];
        for record in records.into_iter() {
            match CharactersMapper::try_from(record) {
                Ok(mapper) => {
                    info!("loaded character mapping: {}", &mapper.comment);
                    mappers.push(mapper);
                }
                Err(e) => {
                    warn!("failed to convert record to mapper: {}, ignored", e);
                }
            }
        }

        Self { mappers }
    }
}

/// Map to target character based on the obfuscation configuration
fn random_char(config: &ObfuscatorConfig, input: char) -> char {
    for mapper in config.mappers.iter() {
        if (mapper.source_start..mapper.source_end).contains(&input) {
            return random_unicode_char(mapper.target_start as u32, mapper.target_end as u32);
        }
    }

    input
}

fn random_unicode_char(start: u32, end: u32) -> char {
    let mut rng = rand::thread_rng();
    let random_value = rng.gen_range(start..=end);
    std::char::from_u32(random_value).unwrap_or('?')
}

pub trait Obfuscator {
    type Output;

    fn obfuscate(&mut self, config: &ObfuscatorConfig);
    fn obfuscated(&self, config: &ObfuscatorConfig) -> Self::Output;
}

impl Obfuscator for serde_json::Map<String, Value> {
    type Output = Self;

    fn obfuscate(&mut self, config: &ObfuscatorConfig) {
        for (_key, value) in self.iter_mut() {
            value.obfuscate(config);
        }
    }

    fn obfuscated(&self, config: &ObfuscatorConfig) -> Self::Output {
        let mut cloned = self.clone();
        cloned.obfuscate(config);

        cloned
    }
}

impl Obfuscator for Value {
    type Output = Self;

    fn obfuscate(&mut self, config: &ObfuscatorConfig) {
        match self {
            Value::String(s) => {
                *s = s.obfuscated(config);
            }
            Value::Object(map) => {
                map.obfuscate(config);
            }
            Value::Array(arr) => {
                for value in arr.iter_mut() {
                    value.obfuscate(config);
                }
            }
            _ => {}
        }
    }

    fn obfuscated(&self, config: &ObfuscatorConfig) -> Self::Output {
        let mut cloned = self.clone();
        cloned.obfuscate(config);

        cloned
    }
}

impl Obfuscator for &mut Tendril<UTF8> {
    type Output = Tendril<UTF8>;

    fn obfuscate(&mut self, config: &ObfuscatorConfig) {
        **self = self.obfuscated(config);
    }

    fn obfuscated(&self, config: &ObfuscatorConfig) -> Self::Output {
        self.chars().map(|c| random_char(config, c)).collect()
    }
}

impl Obfuscator for &mut String {
    type Output = String;

    fn obfuscate(&mut self, config: &ObfuscatorConfig) {
        **self = self.obfuscated(config);
    }

    fn obfuscated(&self, config: &ObfuscatorConfig) -> Self::Output {
        self.chars().map(|c| random_char(config, c)).collect()
    }
}

impl Obfuscator for str {
    type Output = String;

    fn obfuscate(&mut self, _config: &ObfuscatorConfig) {
        todo!()
    }

    fn obfuscated(&self, config: &ObfuscatorConfig) -> Self::Output {
        self.chars().map(|c| random_char(config, c)).collect()
    }
}

impl Obfuscator for char {
    type Output = char;

    fn obfuscate(&mut self, _config: &ObfuscatorConfig) {
        todo!()
    }

    fn obfuscated(&self, config: &ObfuscatorConfig) -> char {
        random_char(config, *self)
    }
}
