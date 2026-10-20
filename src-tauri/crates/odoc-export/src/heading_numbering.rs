use odoc_model::HeadingLevel;

#[derive(Default)]
pub(crate) struct HeadingNumbering {
    counters: [u32; 4],
}

impl HeadingNumbering {
    pub(crate) fn format(&mut self, level: HeadingLevel, content: &str) -> String {
        let index = level_index(level);
        self.counters[index] += 1;
        self.counters[index + 1..].fill(0);
        if has_prefix(level, content) {
            return content.to_owned();
        }

        let number = self.counters[index];
        let prefix = match level {
            HeadingLevel::L1 => format!("{}、", chinese_number(number)),
            HeadingLevel::L2 => format!("（{}）", chinese_number(number)),
            HeadingLevel::L3 => format!("{number}. "),
            HeadingLevel::L4 => format!("（{number}）"),
        };
        format!("{prefix}{content}")
    }
}

fn level_index(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::L1 => 0,
        HeadingLevel::L2 => 1,
        HeadingLevel::L3 => 2,
        HeadingLevel::L4 => 3,
    }
}

fn chinese_number(value: u32) -> String {
    const DIGITS: [&str; 11] = [
        "零", "一", "二", "三", "四", "五", "六", "七", "八", "九", "十",
    ];
    if value > 99 {
        return value.to_string();
    }
    if value <= 10 {
        return DIGITS[value as usize].to_owned();
    }
    if value < 20 {
        return format!("十{}", DIGITS[(value - 10) as usize]);
    }
    let tens = value / 10;
    let ones = value % 10;
    if ones == 0 {
        format!("{}十", DIGITS[tens as usize])
    } else {
        format!("{}十{}", DIGITS[tens as usize], DIGITS[ones as usize])
    }
}

fn has_prefix(level: HeadingLevel, content: &str) -> bool {
    let Some(first) = content.chars().next() else {
        return false;
    };
    match level {
        HeadingLevel::L1 => content.split_once('、').is_some_and(|(number, _)| {
            !number.is_empty() && number.chars().all(is_heading_numeral)
        }),
        HeadingLevel::L2 => content.strip_prefix('（').is_some_and(|rest| {
            rest.split_once('）').is_some_and(|(number, _)| {
                !number.is_empty() && number.chars().all(is_heading_numeral)
            })
        }),
        HeadingLevel::L3 => {
            first.is_ascii_digit()
                && content.split_once('.').is_some_and(|(number, _)| {
                    !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())
                })
        }
        HeadingLevel::L4 => content.strip_prefix('（').is_some_and(|rest| {
            rest.split_once('）').is_some_and(|(number, _)| {
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
            })
        }),
    }
}

fn is_heading_numeral(character: char) -> bool {
    character.is_ascii_digit() || "零〇一二三四五六七八九十百千".contains(character)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_all_levels_and_resets_children() {
        let mut numbering = HeadingNumbering::default();
        assert_eq!(numbering.format(HeadingLevel::L1, "总则"), "一、总则");
        assert_eq!(numbering.format(HeadingLevel::L2, "范围"), "（一）范围");
        assert_eq!(numbering.format(HeadingLevel::L3, "目标"), "1. 目标");
        assert_eq!(numbering.format(HeadingLevel::L4, "措施"), "（1）措施");
        assert_eq!(numbering.format(HeadingLevel::L2, "职责"), "（二）职责");
        assert_eq!(numbering.format(HeadingLevel::L3, "分工"), "1. 分工");
        assert_eq!(numbering.format(HeadingLevel::L1, "附则"), "二、附则");
    }

    #[test]
    fn keeps_imported_heading_prefixes_without_double_numbering() {
        let mut numbering = HeadingNumbering::default();
        assert_eq!(numbering.format(HeadingLevel::L1, "一、总则"), "一、总则");
        assert_eq!(
            numbering.format(HeadingLevel::L2, "（一）范围"),
            "（一）范围"
        );
        assert_eq!(numbering.format(HeadingLevel::L3, "1. 目标"), "1. 目标");
        assert_eq!(numbering.format(HeadingLevel::L4, "（1）措施"), "（1）措施");
        assert_eq!(
            numbering.format(HeadingLevel::L1, "总体要求"),
            "二、总体要求"
        );
    }

    #[test]
    fn chinese_numbers_cover_tens_and_fall_back_after_ninety_nine() {
        for (number, expected) in [
            (10, "十"),
            (11, "十一"),
            (20, "二十"),
            (21, "二十一"),
            (99, "九十九"),
            (100, "100"),
        ] {
            assert_eq!(chinese_number(number), expected);
        }
        assert!(has_prefix(HeadingLevel::L1, "100、章节"));
        assert!(has_prefix(HeadingLevel::L2, "（100）章节"));
    }
}
