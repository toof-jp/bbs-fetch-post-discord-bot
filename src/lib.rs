use std::collections::HashSet;
use std::fmt;

use anyhow::Result;
use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::postgres::PgPool;
use sqlx::FromRow;

#[derive(Debug, Default, Serialize, FromRow)]
pub struct Res {
    pub no: i32,
    pub name_and_trip: String,
    pub datetime: NaiveDateTime,
    pub datetime_text: String,
    pub id: String,
    pub main_text: String,
    pub main_text_html: String,
    pub oekaki_id: Option<i32>,
}

impl fmt::Display for Res {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "### __{} {} {} ID: {}__\n{}\n",
            self.no, self.name_and_trip, self.datetime_text, self.id, self.main_text
        )
    }
}

pub async fn get_res_by_numbers(pool: &PgPool, numbers: Vec<i32>) -> Result<Vec<Res>> {
    if numbers.is_empty() {
        return Ok(Vec::new());
    }

    let query = "SELECT * FROM res WHERE no = ANY($1) ORDER BY no ASC";

    sqlx::query_as::<_, Res>(query)
        .bind(&numbers)
        .fetch_all(pool)
        .await
        .map_err(Into::into)
}

pub async fn get_max_post_number(pool: &PgPool) -> Result<i32> {
    let query = "SELECT MAX(no) FROM res";

    let row: (Option<i32>,) = sqlx::query_as(query).fetch_one(pool).await?;

    Ok(row.0.unwrap_or(0))
}

#[derive(Debug, PartialEq)]
pub enum RangeSpec {
    Include(i32, Option<i32>),
    Exclude(i32, Option<i32>),
    IncludeFrom(i32), // For open-ended ranges like "123-"
    ExcludeFrom(i32), // For open-ended exclusions like "^123-"
    // Relative references (? prefix)
    RelativeInclude(i32, Option<i32>),
    RelativeExclude(i32, Option<i32>),
    RelativeIncludeFrom(i32),
    RelativeExcludeFrom(i32),
}

pub fn parse_range_specifications(input: &str) -> Vec<RangeSpec> {
    let mut specs = Vec::new();
    let parts: Vec<&str> = input.split(',').collect();

    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (is_relative, range_str) = if let Some(stripped) = trimmed.strip_prefix('?') {
            (true, stripped)
        } else {
            (false, trimmed)
        };

        let (is_exclude, range_str) = if let Some(stripped) = range_str.strip_prefix('^') {
            (true, stripped)
        } else {
            (false, range_str)
        };

        if let Some(dash_pos) = range_str.find('-') {
            let start_str = &range_str[..dash_pos];
            let end_str = &range_str[dash_pos + 1..];

            if let Ok(start) = start_str.parse::<i32>() {
                if end_str.is_empty() {
                    // Open-ended range like "123-"
                    match (is_relative, is_exclude) {
                        (true, true) => specs.push(RangeSpec::RelativeExcludeFrom(start)),
                        (true, false) => specs.push(RangeSpec::RelativeIncludeFrom(start)),
                        (false, true) => specs.push(RangeSpec::ExcludeFrom(start)),
                        (false, false) => specs.push(RangeSpec::IncludeFrom(start)),
                    }
                } else if let Ok(end) = end_str.parse::<i32>() {
                    // Closed range like "123-456"
                    match (is_relative, is_exclude) {
                        (true, true) => specs.push(RangeSpec::RelativeExclude(start, Some(end))),
                        (true, false) => specs.push(RangeSpec::RelativeInclude(start, Some(end))),
                        (false, true) => specs.push(RangeSpec::Exclude(start, Some(end))),
                        (false, false) => specs.push(RangeSpec::Include(start, Some(end))),
                    }
                }
            }
        } else if let Ok(num) = range_str.parse::<i32>() {
            match (is_relative, is_exclude) {
                (true, true) => specs.push(RangeSpec::RelativeExclude(num, None)),
                (true, false) => specs.push(RangeSpec::RelativeInclude(num, None)),
                (false, true) => specs.push(RangeSpec::Exclude(num, None)),
                (false, false) => specs.push(RangeSpec::Include(num, None)),
            }
        }
    }

    specs
}

pub fn calculate_post_numbers(specs: Vec<RangeSpec>, max_post_number: i32) -> Vec<i32> {
    let mut included = HashSet::new();
    let mut excluded = HashSet::new();

    // Calculate base for relative references
    // For 123340, we want base 123000 (keep the upper digits)
    let base = if max_post_number > 0 {
        let digits = max_post_number.to_string().len();
        let lower_digits = 3; // We keep the last 3 digits for relative references
        if digits > lower_digits {
            let divisor = 10_i32.pow(lower_digits as u32);
            (max_post_number / divisor) * divisor
        } else {
            0
        }
    } else {
        0
    };

    for spec in specs {
        match spec {
            RangeSpec::Include(start, end) => {
                if let Some(end_num) = end {
                    for i in start..=end_num {
                        included.insert(i);
                    }
                } else {
                    included.insert(start);
                }
            }
            RangeSpec::IncludeFrom(start) => {
                // Include all posts from start to max_post_number
                for i in start..=max_post_number {
                    included.insert(i);
                }
            }
            RangeSpec::Exclude(start, end) => {
                if let Some(end_num) = end {
                    for i in start..=end_num {
                        excluded.insert(i);
                    }
                } else {
                    excluded.insert(start);
                }
            }
            RangeSpec::ExcludeFrom(start) => {
                // Exclude all posts from start to max_post_number
                for i in start..=max_post_number {
                    excluded.insert(i);
                }
            }
            // Relative references
            RangeSpec::RelativeInclude(start, end) => {
                let abs_start = base + start;
                if let Some(end_num) = end {
                    let abs_end = base + end_num;
                    for i in abs_start..=abs_end {
                        included.insert(i);
                    }
                } else {
                    included.insert(abs_start);
                }
            }
            RangeSpec::RelativeIncludeFrom(start) => {
                let abs_start = base + start;
                for i in abs_start..=max_post_number {
                    included.insert(i);
                }
            }
            RangeSpec::RelativeExclude(start, end) => {
                let abs_start = base + start;
                if let Some(end_num) = end {
                    let abs_end = base + end_num;
                    for i in abs_start..=abs_end {
                        excluded.insert(i);
                    }
                } else {
                    excluded.insert(abs_start);
                }
            }
            RangeSpec::RelativeExcludeFrom(start) => {
                let abs_start = base + start;
                for i in abs_start..=max_post_number {
                    excluded.insert(i);
                }
            }
        }
    }

    let mut result: Vec<i32> = included.difference(&excluded).cloned().collect();
    result.sort();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_number() {
        let specs = parse_range_specifications("123");
        assert_eq!(specs, vec![RangeSpec::Include(123, None)]);
    }

    #[test]
    fn test_parse_range() {
        let specs = parse_range_specifications("123-128");
        assert_eq!(specs, vec![RangeSpec::Include(123, Some(128))]);
    }

    #[test]
    fn test_parse_open_range() {
        let specs = parse_range_specifications("123-");
        assert_eq!(specs, vec![RangeSpec::IncludeFrom(123)]);
    }

    #[test]
    fn test_parse_exclusion() {
        let specs = parse_range_specifications("^123");
        assert_eq!(specs, vec![RangeSpec::Exclude(123, None)]);
    }

    #[test]
    fn test_parse_exclusion_range() {
        let specs = parse_range_specifications("^123-128");
        assert_eq!(specs, vec![RangeSpec::Exclude(123, Some(128))]);
    }

    #[test]
    fn test_parse_exclusion_open_range() {
        let specs = parse_range_specifications("^123-");
        assert_eq!(specs, vec![RangeSpec::ExcludeFrom(123)]);
    }

    #[test]
    fn test_parse_relative_single() {
        let specs = parse_range_specifications("?324");
        assert_eq!(specs, vec![RangeSpec::RelativeInclude(324, None)]);
    }

    #[test]
    fn test_parse_relative_range() {
        let specs = parse_range_specifications("?324-326");
        assert_eq!(specs, vec![RangeSpec::RelativeInclude(324, Some(326))]);
    }

    #[test]
    fn test_parse_relative_open_range() {
        let specs = parse_range_specifications("?300-");
        assert_eq!(specs, vec![RangeSpec::RelativeIncludeFrom(300)]);
    }

    #[test]
    fn test_parse_relative_exclusion() {
        let specs = parse_range_specifications("?^325");
        assert_eq!(specs, vec![RangeSpec::RelativeExclude(325, None)]);
    }

    #[test]
    fn test_parse_relative_exclusion_range() {
        let specs = parse_range_specifications("?^325-327");
        assert_eq!(specs, vec![RangeSpec::RelativeExclude(325, Some(327))]);
    }

    #[test]
    fn test_parse_complex_specifications() {
        let specs = parse_range_specifications("10,20-25,30,^23,?324,?^326");
        assert_eq!(
            specs,
            vec![
                RangeSpec::Include(10, None),
                RangeSpec::Include(20, Some(25)),
                RangeSpec::Include(30, None),
                RangeSpec::Exclude(23, None),
                RangeSpec::RelativeInclude(324, None),
                RangeSpec::RelativeExclude(326, None),
            ]
        );
    }

    #[test]
    fn test_parse_empty_and_whitespace() {
        let specs = parse_range_specifications("  123  ,  ,  456  ");
        assert_eq!(
            specs,
            vec![RangeSpec::Include(123, None), RangeSpec::Include(456, None)]
        );
    }

    #[test]
    fn test_calculate_single_number() {
        let specs = vec![RangeSpec::Include(123, None)];
        let result = calculate_post_numbers(specs, 1000);
        assert_eq!(result, vec![123]);
    }

    #[test]
    fn test_calculate_range() {
        let specs = vec![RangeSpec::Include(123, Some(126))];
        let result = calculate_post_numbers(specs, 1000);
        assert_eq!(result, vec![123, 124, 125, 126]);
    }

    #[test]
    fn test_calculate_open_range() {
        let specs = vec![RangeSpec::IncludeFrom(998)];
        let result = calculate_post_numbers(specs, 1000);
        assert_eq!(result, vec![998, 999, 1000]);
    }

    #[test]
    fn test_calculate_with_exclusion() {
        let specs = vec![
            RangeSpec::Include(123, Some(128)),
            RangeSpec::Exclude(126, None),
        ];
        let result = calculate_post_numbers(specs, 1000);
        assert_eq!(result, vec![123, 124, 125, 127, 128]);
    }

    #[test]
    fn test_calculate_with_exclusion_range() {
        let specs = vec![
            RangeSpec::Include(100, Some(110)),
            RangeSpec::Exclude(105, Some(107)),
        ];
        let result = calculate_post_numbers(specs, 1000);
        assert_eq!(result, vec![100, 101, 102, 103, 104, 108, 109, 110]);
    }

    #[test]
    fn test_calculate_relative_reference() {
        let specs = vec![RangeSpec::RelativeInclude(324, None)];
        let result = calculate_post_numbers(specs, 123340);
        assert_eq!(result, vec![123324]);
    }

    #[test]
    fn test_calculate_relative_range() {
        let specs = vec![RangeSpec::RelativeInclude(324, Some(326))];
        let result = calculate_post_numbers(specs, 123340);
        assert_eq!(result, vec![123324, 123325, 123326]);
    }

    #[test]
    fn test_calculate_relative_with_exclusion() {
        let specs = vec![
            RangeSpec::RelativeInclude(320, Some(330)),
            RangeSpec::RelativeExclude(325, None),
        ];
        let result = calculate_post_numbers(specs, 123340);
        let expected: Vec<i32> = (123320..=123330).filter(|&x| x != 123325).collect();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_calculate_relative_open_range() {
        let specs = vec![RangeSpec::RelativeIncludeFrom(338)];
        let result = calculate_post_numbers(specs, 123340);
        assert_eq!(result, vec![123338, 123339, 123340]);
    }

    #[test]
    fn test_calculate_complex_mix() {
        let specs = vec![
            RangeSpec::Include(100, Some(105)),
            RangeSpec::RelativeInclude(324, None),
            RangeSpec::Exclude(102, None),
            RangeSpec::RelativeExclude(324, None),
        ];
        let result = calculate_post_numbers(specs, 123340);
        assert_eq!(result, vec![100, 101, 103, 104, 105]);
    }

    #[test]
    fn test_calculate_with_small_max_number() {
        // Test when max_post_number is less than 1000
        let specs = vec![RangeSpec::RelativeInclude(324, None)];
        let result = calculate_post_numbers(specs, 500);
        assert_eq!(result, vec![324]); // Base is 0, so relative becomes absolute
    }

    #[test]
    fn test_calculate_edge_cases() {
        // Test with max_post_number of 0
        let specs = vec![RangeSpec::Include(1, Some(3))];
        let result = calculate_post_numbers(specs, 0);
        assert_eq!(result, vec![1, 2, 3]);

        // Test empty specs
        let specs = vec![];
        let result = calculate_post_numbers(specs, 1000);
        assert_eq!(result, vec![] as Vec<i32>);
    }

    #[test]
    fn test_calculate_overlapping_ranges() {
        let specs = vec![
            RangeSpec::Include(1, Some(10)),
            RangeSpec::Include(5, Some(15)),
            RangeSpec::Exclude(8, Some(12)),
        ];
        let result = calculate_post_numbers(specs, 100);
        let expected: Vec<i32> = vec![1, 2, 3, 4, 5, 6, 7, 13, 14, 15];
        assert_eq!(result, expected);
    }
}
