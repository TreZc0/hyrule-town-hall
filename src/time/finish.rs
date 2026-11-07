use std::time::Duration;

/// Parses an elapsed H:MM:SS time with optional fractional seconds (up to milliseconds).
pub(crate) fn parse(input: &str) -> Option<Duration> {
    let mut parts = input.trim().split(':');
    let hours = parts.next()?;
    let minutes = parts.next()?;
    let seconds = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let (seconds, fraction) = seconds
        .split_once('.')
        .map_or((seconds, None), |(s, f)| (s, Some(f)));
    let number = |part: &str| -> Option<u64> {
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };
    let hours = number(hours)?;
    let minutes = number(minutes)?;
    let seconds = number(seconds)?;
    if minutes >= 60 || seconds >= 60 {
        return None;
    }
    let millis = match fraction {
        Some(fraction) if (1..=3).contains(&fraction.len()) => {
            number(fraction)? * 10_u64.pow(3 - fraction.len() as u32)
        }
        Some(_) => return None,
        None => 0,
    };
    let seconds = hours
        .checked_mul(3600)?
        .checked_add(minutes * 60)?
        .checked_add(seconds)?;
    Duration::from_secs(seconds).checked_add(Duration::from_millis(millis))
}

pub(crate) fn format_millis(time: Duration) -> String {
    let seconds = time.as_secs();
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        seconds / 3600,
        seconds % 3600 / 60,
        seconds % 60,
        time.subsec_millis(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_whole_seconds_and_milliseconds() {
        for (input, millis) in [
            ("1:23:45", 0),
            ("01:23:45.000", 0),
            ("01:23:45.1", 100),
            ("01:23:45.01", 10),
            (" 01:23:45.001 ", 1),
            ("01:23:45.999", 999),
        ] {
            assert_eq!(
                parse(input),
                Some(Duration::from_millis(5_025_000 + millis)),
                "{input}"
            );
        }
        assert_eq!(
            parse("25:00:00.001"),
            Some(Duration::from_millis(90_000_001))
        );
    }

    #[test]
    fn distinguishes_finishes_one_millisecond_apart() {
        let faster = parse("01:23:45.123").unwrap();
        let slower = parse("01:23:45.124").unwrap();
        assert!(faster < slower);
        assert!(slower > faster);
        assert_eq!(slower - faster, Duration::from_millis(1));
        assert!(parse("01:23:45.999").unwrap() < parse("01:23:46").unwrap());
    }

    #[test]
    fn equivalent_inputs_remain_tied() {
        assert_eq!(parse("1:23:45"), parse("01:23:45.000"));
        assert_eq!(parse("1:23:45.1"), parse("01:23:45.100"));
    }

    #[test]
    fn rejects_invalid_or_overflowing_times() {
        for input in [
            "",
            "1:23",
            "1:23:45:00",
            "-1:23:45",
            "1:-23:45",
            "1:23:-45",
            "+1:23:45",
            "1:60:00",
            "1:00:60",
            "1:23:45.",
            "1:23:45.1234",
            "1:23:45.1.2",
            "1:23:45.abc",
            "1:23:45.+1",
            "1:23:45. 1",
            "18446744073709551615:00:00",
            "18446744073709551616:00:00",
        ] {
            assert_eq!(parse(input), None, "{input}");
        }
    }

    #[test]
    fn displays_milliseconds_including_leading_zeroes() {
        for input in [
            "00:00:00.000",
            "01:23:45.001",
            "01:23:45.120",
            "25:00:00.999",
        ] {
            assert_eq!(format_millis(parse(input).unwrap()), input);
        }
    }
}
