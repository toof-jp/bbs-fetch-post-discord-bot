use std::collections::HashSet;
use std::fmt;

use anyhow::Result;
use chrono::NaiveDateTime;
use log::{debug, error, trace};
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
        debug!("get_res_by_numbers: empty numbers array");
        return Ok(Vec::new());
    }

    let query = "SELECT * FROM res WHERE no = ANY($1) ORDER BY no ASC";
    debug!("get_res_by_numbers: querying for numbers: {numbers:?}");

    let result = sqlx::query_as::<_, Res>(query)
        .bind(&numbers)
        .fetch_all(pool)
        .await
        .map_err(Into::into);

    match &result {
        Ok(posts) => debug!("get_res_by_numbers: found {} posts", posts.len()),
        Err(e) => error!("get_res_by_numbers: error: {e:?}"),
    }

    result
}

pub async fn get_max_post_number(pool: &PgPool) -> Result<i32> {
    let query = "SELECT MAX(no) FROM res";
    debug!("get_max_post_number: executing query");

    let row: (Option<i32>,) = sqlx::query_as(query).fetch_one(pool).await?;
    let result = row.0.unwrap_or(0);
    debug!("get_max_post_number: result = {result}");

    Ok(result)
}

#[derive(Debug, PartialEq)]
pub enum RangeSpec {
    Include(i32, Option<i32>),
    Exclude(i32, Option<i32>),
    IncludeFrom(i32), // For open-ended ranges like "123-"
    ExcludeFrom(i32), // For open-ended exclusions like "^123-"
    // Relative references (? prefix) with digit count
    RelativeInclude(i32, Option<i32>, usize), // (start, end, digit_count)
    RelativeExclude(i32, Option<i32>, usize), // (start, end, digit_count)
    RelativeIncludeFrom(i32, usize),          // (start, digit_count)
    RelativeExcludeFrom(i32, usize),          // (start, digit_count)
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
                let digit_count = if is_relative { start_str.len() } else { 0 };

                if end_str.is_empty() {
                    // Open-ended range like "123-"
                    match (is_relative, is_exclude) {
                        (true, true) => {
                            specs.push(RangeSpec::RelativeExcludeFrom(start, digit_count))
                        }
                        (true, false) => {
                            specs.push(RangeSpec::RelativeIncludeFrom(start, digit_count))
                        }
                        (false, true) => specs.push(RangeSpec::ExcludeFrom(start)),
                        (false, false) => specs.push(RangeSpec::IncludeFrom(start)),
                    }
                } else if let Ok(end) = end_str.parse::<i32>() {
                    // Closed range like "123-456"
                    match (is_relative, is_exclude) {
                        (true, true) => {
                            specs.push(RangeSpec::RelativeExclude(start, Some(end), digit_count))
                        }
                        (true, false) => {
                            specs.push(RangeSpec::RelativeInclude(start, Some(end), digit_count))
                        }
                        (false, true) => specs.push(RangeSpec::Exclude(start, Some(end))),
                        (false, false) => specs.push(RangeSpec::Include(start, Some(end))),
                    }
                }
            }
        } else if let Ok(num) = range_str.parse::<i32>() {
            let digit_count = if is_relative { range_str.len() } else { 0 };

            match (is_relative, is_exclude) {
                (true, true) => specs.push(RangeSpec::RelativeExclude(num, None, digit_count)),
                (true, false) => specs.push(RangeSpec::RelativeInclude(num, None, digit_count)),
                (false, true) => specs.push(RangeSpec::Exclude(num, None)),
                (false, false) => specs.push(RangeSpec::Include(num, None)),
            }
        }
    }

    specs
}

pub fn calculate_post_numbers(specs: Vec<RangeSpec>, max_post_number: i32) -> Vec<i32> {
    debug!(
        "calculate_post_numbers called with specs: {specs:?}, max_post_number: {max_post_number}"
    );
    let mut included = HashSet::new();
    let mut excluded = HashSet::new();

    // Helper function to calculate absolute post number from relative reference
    let calculate_absolute = |relative_num: i32, digit_count: usize| -> i32 {
        if max_post_number <= 0 {
            return relative_num;
        }

        let divisor = 10_i32.pow(digit_count as u32);
        let base = (max_post_number / divisor) * divisor;
        let candidate = base + relative_num;

        // If the candidate exceeds max, go to previous base
        let result = if candidate > max_post_number {
            let prev_base = base - divisor;
            if prev_base >= 0 {
                prev_base + relative_num
            } else {
                relative_num // Fallback to just the relative number
            }
        } else {
            candidate
        };

        trace!("calculate_absolute: max={max_post_number}, relative={relative_num}, digits={digit_count}, divisor={divisor}, base={base}, candidate={candidate}, result={result}");

        result
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
            RangeSpec::RelativeInclude(start, end, digit_count) => {
                debug!(
                    "Processing RelativeInclude: start={start}, end={end:?}, digit_count={digit_count}"
                );
                let abs_start = calculate_absolute(start, digit_count);
                if let Some(end_num) = end {
                    let abs_end = calculate_absolute(end_num, digit_count);
                    debug!("Including range {abs_start}..={abs_end}");
                    for i in abs_start..=abs_end {
                        included.insert(i);
                    }
                } else {
                    debug!("Including single number {abs_start}");
                    included.insert(abs_start);
                }
            }
            RangeSpec::RelativeIncludeFrom(start, digit_count) => {
                let abs_start = calculate_absolute(start, digit_count);
                for i in abs_start..=max_post_number {
                    included.insert(i);
                }
            }
            RangeSpec::RelativeExclude(start, end, digit_count) => {
                debug!(
                    "Processing RelativeExclude: start={start}, end={end:?}, digit_count={digit_count}"
                );
                let abs_start = calculate_absolute(start, digit_count);
                if let Some(end_num) = end {
                    let abs_end = calculate_absolute(end_num, digit_count);
                    debug!("Excluding range {abs_start}..={abs_end}");
                    for i in abs_start..=abs_end {
                        excluded.insert(i);
                    }
                } else {
                    debug!("Excluding single number {abs_start}");
                    excluded.insert(abs_start);
                }
            }
            RangeSpec::RelativeExcludeFrom(start, digit_count) => {
                let abs_start = calculate_absolute(start, digit_count);
                for i in abs_start..=max_post_number {
                    excluded.insert(i);
                }
            }
        }
    }

    let mut result: Vec<i32> = included.difference(&excluded).cloned().collect();
    result.sort();
    debug!("Final result - included: {included:?}, excluded: {excluded:?}, result: {result:?}");
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
        assert_eq!(specs, vec![RangeSpec::RelativeInclude(324, None, 3)]);
    }

    #[test]
    fn test_parse_relative_range() {
        let specs = parse_range_specifications("?324-326");
        assert_eq!(specs, vec![RangeSpec::RelativeInclude(324, Some(326), 3)]);
    }

    #[test]
    fn test_parse_relative_open_range() {
        let specs = parse_range_specifications("?300-");
        assert_eq!(specs, vec![RangeSpec::RelativeIncludeFrom(300, 3)]);
    }

    #[test]
    fn test_parse_relative_exclusion() {
        let specs = parse_range_specifications("?^325");
        assert_eq!(specs, vec![RangeSpec::RelativeExclude(325, None, 3)]);
    }

    #[test]
    fn test_parse_relative_exclusion_range() {
        let specs = parse_range_specifications("?^325-327");
        assert_eq!(specs, vec![RangeSpec::RelativeExclude(325, Some(327), 3)]);
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
                RangeSpec::RelativeInclude(324, None, 3),
                RangeSpec::RelativeExclude(326, None, 3),
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
        let specs = vec![RangeSpec::RelativeInclude(324, None, 3)];
        let result = calculate_post_numbers(specs, 123340);
        assert_eq!(result, vec![123324]);
    }

    #[test]
    fn test_calculate_relative_range() {
        let specs = vec![RangeSpec::RelativeInclude(324, Some(326), 3)];
        let result = calculate_post_numbers(specs, 123340);
        assert_eq!(result, vec![123324, 123325, 123326]);
    }

    #[test]
    fn test_calculate_relative_with_exclusion() {
        let specs = vec![
            RangeSpec::RelativeInclude(320, Some(330), 3),
            RangeSpec::RelativeExclude(325, None, 3),
        ];
        let result = calculate_post_numbers(specs, 123340);
        let expected: Vec<i32> = (123320..=123330).filter(|&x| x != 123325).collect();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_calculate_relative_open_range() {
        let specs = vec![RangeSpec::RelativeIncludeFrom(338, 3)];
        let result = calculate_post_numbers(specs, 123340);
        assert_eq!(result, vec![123338, 123339, 123340]);
    }

    #[test]
    fn test_calculate_complex_mix() {
        let specs = vec![
            RangeSpec::Include(100, Some(105)),
            RangeSpec::RelativeInclude(324, None, 3),
            RangeSpec::Exclude(102, None),
            RangeSpec::RelativeExclude(324, None, 3),
        ];
        let result = calculate_post_numbers(specs, 123340);
        assert_eq!(result, vec![100, 101, 103, 104, 105]);
    }

    #[test]
    fn test_calculate_with_small_max_number() {
        // Test when max_post_number is less than 1000
        let specs = vec![RangeSpec::RelativeInclude(324, None, 3)];
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

    #[test]
    fn test_parse_relative_with_different_digits() {
        // Test 2 digits
        let specs = parse_range_specifications("?24");
        assert_eq!(specs, vec![RangeSpec::RelativeInclude(24, None, 2)]);

        // Test 4 digits
        let specs = parse_range_specifications("?1234");
        assert_eq!(specs, vec![RangeSpec::RelativeInclude(1234, None, 4)]);

        // Test mixed digits
        let specs = parse_range_specifications("?24,?324,?1234");
        assert_eq!(
            specs,
            vec![
                RangeSpec::RelativeInclude(24, None, 2),
                RangeSpec::RelativeInclude(324, None, 3),
                RangeSpec::RelativeInclude(1234, None, 4),
            ]
        );
    }

    #[test]
    fn test_calculate_relative_with_different_digits() {
        // Test with max 123456 and different digit counts
        let max_post = 123456;

        // ?56 with 2 digits: 123456 / 100 * 100 + 56 = 123456
        let specs = vec![RangeSpec::RelativeInclude(56, None, 2)];
        let result = calculate_post_numbers(specs, max_post);
        assert_eq!(result, vec![123456]);

        // ?456 with 3 digits: 123456 / 1000 * 1000 + 456 = 123456
        let specs = vec![RangeSpec::RelativeInclude(456, None, 3)];
        let result = calculate_post_numbers(specs, max_post);
        assert_eq!(result, vec![123456]);

        // ?3456 with 4 digits: 123456 / 10000 * 10000 + 3456 = 123456
        let specs = vec![RangeSpec::RelativeInclude(3456, None, 4)];
        let result = calculate_post_numbers(specs, max_post);
        assert_eq!(result, vec![123456]);

        // Test with different values
        // ?24 with 2 digits: 123456 / 100 * 100 + 24 = 123424
        let specs = vec![RangeSpec::RelativeInclude(24, None, 2)];
        let result = calculate_post_numbers(specs, max_post);
        assert_eq!(result, vec![123424]);
    }

    #[test]
    fn test_calculate_relative_with_wraparound() {
        // Test the specific case mentioned: max 2345, ?456 should return 1456
        let max_post = 2345;

        // ?456 with max 2345: since 2456 > 2345, should wrap to 1456
        let specs = vec![RangeSpec::RelativeInclude(456, None, 3)];
        let result = calculate_post_numbers(specs, max_post);
        assert_eq!(result, vec![1456]);

        // ?345 with max 2345: should return 2345 (exact match)
        let specs = vec![RangeSpec::RelativeInclude(345, None, 3)];
        let result = calculate_post_numbers(specs, max_post);
        assert_eq!(result, vec![2345]);

        // ?100 with max 2345: should return 2100
        let specs = vec![RangeSpec::RelativeInclude(100, None, 3)];
        let result = calculate_post_numbers(specs, max_post);
        assert_eq!(result, vec![2100]);

        // Test with smaller max
        let max_post = 456;

        // ?456 with max 456: should return 456 (exact match)
        let specs = vec![RangeSpec::RelativeInclude(456, None, 3)];
        let result = calculate_post_numbers(specs, max_post);
        assert_eq!(result, vec![456]);

        // ?500 with max 456: since we can't go negative, should return 500
        let specs = vec![RangeSpec::RelativeInclude(500, None, 3)];
        let result = calculate_post_numbers(specs, max_post);
        assert_eq!(result, vec![500]);
    }
}
