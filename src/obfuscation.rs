use html5ever::tendril::{fmt::UTF8, Tendril};
use serde_json::Value;
use std::ops::RangeInclusive;

// 常见汉字范围
const CHINESE_RANGE: RangeInclusive<char> = '\u{4e00}'..='\u{9fa5}';

// 根据输入生成随机字符
fn random_char(input: char) -> char {
    if CHINESE_RANGE.contains(&input) {
        // 仅对常见汉字范围进行替换，忽略其它字符
        let c = rand::random::<u32>() % (0x9fa5 - 0x4e00) + 0x4e00;
        std::char::from_u32(c).unwrap_or('?')
    } else {
        input
    }
}

pub trait Obfuscator {
    type Output;

    fn obfuscate(&mut self);
    fn obfuscated(&self) -> Self::Output;
}

impl Obfuscator for serde_json::Map<String, Value> {
    type Output = Self;

    fn obfuscate(&mut self) {
        for (_key, value) in self.iter_mut() {
            value.obfuscate();
        }
    }

    fn obfuscated(&self) -> Self::Output {
        let mut cloned = self.clone();
        cloned.obfuscate();

        cloned
    }
}

impl Obfuscator for Value {
    type Output = Self;

    fn obfuscate(&mut self) {
        match self {
            Value::String(s) => {
                *s = s.obfuscated();
            }
            Value::Object(map) => {
                map.obfuscate();
            }
            Value::Array(arr) => {
                for value in arr.iter_mut() {
                    value.obfuscate();
                }
            }
            _ => {}
        }
    }

    fn obfuscated(&self) -> Self::Output {
        let mut cloned = self.clone();
        cloned.obfuscate();

        cloned
    }
}

impl Obfuscator for &mut Tendril<UTF8> {
    type Output = Tendril<UTF8>;

    fn obfuscate(&mut self) {
        **self = self.obfuscated();
    }

    fn obfuscated(&self) -> Self::Output {
        self.chars().map(random_char).collect()
    }
}

impl Obfuscator for &mut String {
    type Output = String;

    fn obfuscate(&mut self) {
        **self = self.obfuscated();
    }

    fn obfuscated(&self) -> Self::Output {
        self.chars().map(random_char).collect()
    }
}
