use std::ops::RangeInclusive;

use html5ever::tendril::{fmt::UTF8, Tendril};

// 常见汉字范围
const CHINESE_RANGE: RangeInclusive<char> = '\u{4e00}'..='\u{9fa5}';

/// 对输入文本进行混淆，返回混淆后的文本
pub fn obfuscate_text(text: &mut Tendril<UTF8>) -> Tendril<UTF8> {
    text.chars().map(random_char).collect()
}

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
