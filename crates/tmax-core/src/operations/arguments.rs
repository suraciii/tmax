use crate::{Error, Request, Result};

pub(crate) fn arg<'a>(r: &'a Request, key: &str) -> Option<&'a str> {
    r.arguments
        .get(key)
        .and_then(|v| v.first())
        .map(String::as_str)
}
pub(crate) fn args<'a>(r: &'a Request, key: &str) -> &'a [String] {
    r.arguments.get(key).map(Vec::as_slice).unwrap_or(&[])
}
pub(crate) fn operand(r: &Request, n: usize) -> Result<&str> {
    r.operands.get(n).map(String::as_str).ok_or_else(|| {
        Error::new(
            "invalid_arguments",
            "A required positional operand is missing",
        )
    })
}
pub(crate) fn required<'a>(r: &'a Request, key: &str) -> Result<&'a str> {
    arg(r, key).ok_or_else(|| {
        Error::new("invalid_arguments", "A required named option is missing").field(key)
    })
}
pub(crate) fn boolean(s: &str) -> Result<bool> {
    match s {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(Error::new(
            "invalid_arguments",
            "Expected exactly true or false",
        )),
    }
}
pub(crate) fn duration(s: &str) -> Result<i64> {
    let unit = s.as_bytes().last().copied().ok_or_else(|| {
        Error::new(
            "invalid_arguments",
            "Duration requires a positive integer and unit",
        )
    })?;
    let factor = match unit {
        b's' => 1,
        b'm' => 60,
        b'h' => 3600,
        b'd' => 86400,
        _ => {
            return Err(Error::new(
                "invalid_arguments",
                "Duration unit must be s, m, h, or d",
            ));
        }
    };
    let digits = &s[..s.len() - 1];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::new(
            "invalid_arguments",
            "Duration requires a positive integer and s, m, h, or d",
        ));
    }
    let n: i64 = digits.parse().map_err(|_| {
        Error::new(
            "invalid_arguments",
            "Duration exceeds the supported integer range",
        )
    })?;
    n.checked_mul(factor).filter(|n| *n > 0).ok_or_else(|| {
        Error::new(
            "invalid_arguments",
            "Duration must be positive and within signed 64-bit seconds",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_duration_and_no_document_input() {
        assert_eq!(duration("1d").unwrap(), 86400);
        for bad in ["0s", "1.5h", "1h30m", "10", "1H", "-1s"] {
            assert!(duration(bad).is_err());
        }
    }
}
