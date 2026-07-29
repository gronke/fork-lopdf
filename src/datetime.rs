use super::Object;

#[cfg(feature = "chrono")]
mod chrono_impl {
    use crate::{Object, datetime::convert_utc_offset};
    use chrono::prelude::*;

    impl From<DateTime<Local>> for Object {
        fn from(date: DateTime<Local>) -> Self {
            let mut timezone_str = date.format("D:%Y%m%d%H%M%S%:z'").to_string().into_bytes();
            convert_utc_offset(&mut timezone_str);
            Object::string_literal(timezone_str)
        }
    }

    impl From<DateTime<Utc>> for Object {
        fn from(date: DateTime<Utc>) -> Self {
            Object::string_literal(date.format("D:%Y%m%d%H%M%SZ").to_string())
        }
    }

    impl TryFrom<super::DateTime> for DateTime<Local> {
        type Error = chrono::format::ParseError;

        fn try_from(value: super::DateTime) -> Result<DateTime<Local>, Self::Error> {
            let from_date = |date: NaiveDate| {
                FixedOffset::east_opt(0)
                    .unwrap()
                    .from_utc_datetime(&date.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap()))
            };

            DateTime::parse_from_str(&value.0, "%Y%m%d%H%M%S%#z")
                .or_else(|_| DateTime::parse_from_str(&value.0, "%Y%m%d%H%M%#z"))
                .or_else(|_| NaiveDate::parse_from_str(&value.0, "%Y%m%d").map(from_date))
                .map(|date| date.with_timezone(&Local))
        }
    }
}

#[cfg(feature = "jiff")]
mod jiff_impl {
    use crate::{Object, datetime::convert_utc_offset};
    use jiff::{Timestamp, Zoned};

    impl From<Zoned> for Object {
        fn from(date: Zoned) -> Self {
            let mut timezone_str = date.strftime("D:%Y%m%d%H%M%S%:z'").to_string().into_bytes();
            convert_utc_offset(&mut timezone_str);
            Object::string_literal(timezone_str)
        }
    }

    impl From<Timestamp> for Object {
        fn from(date: Timestamp) -> Self {
            Object::string_literal(date.strftime("D:%Y%m%d%H%M%SZ").to_string())
        }
    }

    impl TryFrom<super::DateTime> for Zoned {
        type Error = jiff::Error;

        fn try_from(value: super::DateTime) -> Result<Self, Self::Error> {
            use jiff::civil::{Date, DateTime};

            // We attempt to parse different date time formats based on Section 7.9.4 "Dates" in
            // PDF 32000-1:2008 here.
            //
            // "A PLUS SIGN as the value of the O field signifies that the local time is later than
            // UT, a HYPHEN-MINUS signifies that local time is earlier than UT, and the LATIN
            // CAPITAL Z signifies that local time is equal to UT. If no UT information is
            // specified, the relationship of the specified time to UT shall be considered GMT."
            //
            // 1. Try parsing the full date and time with the `%#z` specifier to parse the timezone as a `Zoned` object.
            // 2. Try parsing the full date and time with the 'Z' suffix as a `DateTime` interpreted to be in the UTC
            //    timezone.
            // 3. Try parsing the date and time without the seconds specified with the `%#z` specifier to parse the
            //    timezone as a `Zoned` object.
            // 4. Try parsing the date and time without the seconds specified with the 'Z' as a `DateTime` interpreted
            //    to be in the UTC timezone.
            // 5. Try parsing the date with no time as a `Date` interpreted to be in the GMT timezone.
            //
            // In all cases we return a `Zoned` object here to preserve the timezone.
            Zoned::strptime("%Y%m%d%H%M%S%#z", &value.0)
                .or_else(|_| DateTime::strptime("%Y%m%d%H%M%SZ", &value.0).and_then(|dt| dt.in_tz("UTC")))
                .or_else(|_| Zoned::strptime("%Y%m%d%H%M%#z", &value.0))
                .or_else(|_| DateTime::strptime("%Y%m%d%H%MZ", &value.0).and_then(|dt| dt.in_tz("UTC")))
                .or_else(|_| Date::strptime("%Y%m%d", &value.0).and_then(|dt| dt.at(0, 0, 0, 0).in_tz("GMT")))
        }
    }
}

#[cfg(feature = "time")]
mod time_impl {
    use crate::Object;
    use time::{OffsetDateTime, PrimitiveDateTime};

    /// The naive datetime is taken to be UTC and rendered with the `Z` suffix
    /// (PDF 32000-1 §7.9.4) — the `time`-crate counterpart of the
    /// `chrono::DateTime<Utc>` and `jiff::Timestamp` impls above.
    ///
    /// (This replaces a `From<time::Time>` impl that never compiled — see
    /// issue #518: it referenced a nonexistent `FormatItem::StringLiteral`
    /// with a strftime pattern, and a bare time-of-day cannot form a valid
    /// PDF date anyway, which must start at the year.)
    impl From<PrimitiveDateTime> for Object {
        fn from(date: PrimitiveDateTime) -> Self {
            Object::string_literal({
                // D:%Y%m%d%H%M%SZ
                let format =
                    time::format_description::parse_borrowed::<3>("D:[year][month][day][hour][minute][second]Z")
                        .unwrap();
                date.format(&format).unwrap()
            })
        }
    }

    impl From<OffsetDateTime> for Object {
        fn from(date: OffsetDateTime) -> Self {
            Object::string_literal({
                // D:%Y%m%d%H%M%S:%z'
                let format = time::format_description::parse_borrowed::<3>(
                    "D:[year][month][day][hour][minute][second][offset_hour sign:mandatory]'[offset_minute]'",
                )
                .unwrap();
                date.format(&format).unwrap()
            })
        }
    }

    /// WARNING: `tm_wday` (weekday), `tm_yday` (day index in year), `tm_isdst`
    /// (daylight saving time) and `tm_nsec` (nanoseconds of the date from 1970)
    /// are set to 0 since they aren't available in the PDF time format. They could,
    /// however, be calculated manually
    impl TryFrom<super::DateTime> for OffsetDateTime {
        type Error = time::Error;

        fn try_from(value: super::DateTime) -> Result<OffsetDateTime, Self::Error> {
            let format = time::format_description::parse_borrowed::<3>(
                "[year][month][day][hour][minute][second][offset_hour sign:mandatory][offset_minute]",
            )
            .unwrap();

            Ok(OffsetDateTime::parse(&value.0, &format)?)
        }
    }
}

// Find the last `:` and turn it into an `'` to account for PDF weirdness
#[allow(dead_code)]
fn convert_utc_offset(bytes: &mut [u8]) {
    let mut index = bytes.len();
    while let Some(last) = bytes[..index].last_mut() {
        if *last == b':' {
            *last = b'\'';
            break;
        }
        index -= 1;
    }
}

#[derive(Clone, Debug)]
pub struct DateTime(String);

impl Object {
    // Parses the `D`, `:` and `\` out of a `Object::String` to parse the date time
    fn datetime_string(&self) -> Option<String> {
        if let Object::String(bytes, _) = self {
            String::from_utf8(bytes.iter().filter(|b| !b"D:'".contains(b)).cloned().collect()).ok()
        } else {
            None
        }
    }

    pub fn as_datetime(&self) -> Option<DateTime> {
        self.datetime_string().map(DateTime)
    }
}

/// Read `count` digits at `at`, advancing it only on success. `None` when the
/// field is absent or is not all digits, which is how the optional trailing
/// fields of a PDF date end.
fn take_digits(bytes: &[u8], at: &mut usize, count: usize) -> Option<u32> {
    let end = at.checked_add(count)?;
    let slice = bytes.get(*at..end)?;
    if !slice.iter().all(u8::is_ascii_digit) {
        return None;
    }
    *at = end;
    Some(slice.iter().fold(0, |acc, b| acc * 10 + u32::from(b - b'0')))
}

/// A PDF date in its own fields, available without a date library.
///
/// Every other conversion in this module needs `chrono`, `jiff` or `time`.
/// The default features supply all three, but a `default-features = false`
/// build — which is what a crate embedding lopdf in a wasm target or keeping
/// its dependency tree small will use — can still reach
/// [`Object::as_datetime`] and receives a [`DateTime`] whose inner string is
/// private. There is then no way to read the date at all, and no way to write
/// one back. This type closes that gap with arithmetic the format already
/// implies.
///
/// The offset is kept as the file wrote it rather than normalised to UTC or
/// to the local zone, so a date read and written again says the same thing.
///
/// Parsing follows ISO 32000-1, 7.9.4: only the year is required and the
/// remaining fields default to `01` for month and day and `00` for the time.
/// Values outside their range are rejected rather than clamped, matching the
/// strictness of the `chrono`, `jiff` and `time` conversions above.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PdfDateTime {
    pub year: u16,
    /// 1 to 12.
    pub month: u8,
    /// 1 to 31. Not checked against the month's length.
    pub day: u8,
    /// 0 to 23.
    pub hour: u8,
    /// 0 to 59.
    pub minute: u8,
    /// 0 to 59.
    pub second: u8,
    /// Minutes east of UT. `None` when the date carried no offset, which
    /// 7.9.4 says is to be read as GMT. A `Z` suffix reads as `Some(0)`.
    pub utc_offset_minutes: Option<i16>,
}

impl TryFrom<DateTime> for PdfDateTime {
    type Error = crate::Error;

    fn try_from(value: DateTime) -> Result<Self, Self::Error> {
        // `datetime_string` has already removed the `D:` prefix and the
        // apostrophes, so `D:20260710143000+02'00'` arrives as
        // `20260710143000+0200`.
        let bytes = value.0.as_bytes();
        let malformed = || crate::Error::Syntax(format!("invalid PDF date {:?}", value.0));
        let mut at = 0;

        let year = take_digits(bytes, &mut at, 4).ok_or_else(malformed)?;
        // Each remaining pair is optional, in order; the first one absent
        // ends the date.
        let month = take_digits(bytes, &mut at, 2).unwrap_or(1);
        let day = take_digits(bytes, &mut at, 2).unwrap_or(1);
        let hour = take_digits(bytes, &mut at, 2).unwrap_or(0);
        let minute = take_digits(bytes, &mut at, 2).unwrap_or(0);
        let second = take_digits(bytes, &mut at, 2).unwrap_or(0);

        let utc_offset_minutes = match bytes.get(at) {
            None => None,
            Some(b'Z') => Some(0),
            Some(sign @ (b'+' | b'-')) => {
                let east = *sign == b'+';
                at += 1;
                let offset_hour = take_digits(bytes, &mut at, 2).ok_or_else(malformed)?;
                let offset_minute = take_digits(bytes, &mut at, 2).unwrap_or(0);
                if offset_hour > 23 || offset_minute > 59 {
                    return Err(malformed());
                }
                let total = (offset_hour * 60 + offset_minute) as i16;
                Some(if east { total } else { -total })
            }
            Some(_) => return Err(malformed()),
        };

        if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 59 {
            return Err(malformed());
        }
        Ok(PdfDateTime {
            year: year as u16,
            month: month as u8,
            day: day as u8,
            hour: hour as u8,
            minute: minute as u8,
            second: second as u8,
            utc_offset_minutes,
        })
    }
}

impl std::fmt::Display for PdfDateTime {
    /// The PDF form. An absent offset stays absent; `Some(0)` writes as
    /// `+00'00'`, which says the same as the `Z` it may have been read from.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "D:{:04}{:02}{:02}{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )?;
        if let Some(offset) = self.utc_offset_minutes {
            let sign = if offset < 0 { '-' } else { '+' };
            let magnitude = offset.unsigned_abs();
            write!(f, "{sign}{:02}'{:02}'", magnitude / 60, magnitude % 60)?;
        }
        Ok(())
    }
}

impl From<PdfDateTime> for Object {
    fn from(date: PdfDateTime) -> Self {
        Object::string_literal(date.to_string())
    }
}

#[cfg(test)]
mod pdf_datetime_tests {
    use super::*;

    fn parse(text: &str) -> crate::Result<PdfDateTime> {
        Object::string_literal(text)
            .as_datetime()
            .expect("a literal string is a datetime candidate")
            .try_into()
    }

    #[test]
    fn a_full_date_keeps_the_offset_it_was_written_with() {
        let date = parse("D:20260710143000+02'00'").unwrap();
        assert_eq!(
            date,
            PdfDateTime {
                year: 2026,
                month: 7,
                day: 10,
                hour: 14,
                minute: 30,
                second: 0,
                utc_offset_minutes: Some(120),
            }
        );
        // Not shifted to UTC, and not to the local zone either.
        assert_eq!(date.to_string(), "D:20260710143000+02'00'");
    }

    #[test]
    fn a_western_offset_is_negative() {
        let date = parse("D:20260710143000-05'30'").unwrap();
        assert_eq!(date.utc_offset_minutes, Some(-330));
        assert_eq!(date.to_string(), "D:20260710143000-05'30'");
    }

    #[test]
    fn absent_fields_take_their_documented_defaults() {
        // 7.9.4: only the year is required; month and day default to 01 and
        // the time to 00.
        assert_eq!(parse("D:2026").unwrap().to_string(), "D:20260101000000");
        assert_eq!(parse("D:202607").unwrap().to_string(), "D:20260701000000");
        assert_eq!(parse("D:20260710").unwrap().to_string(), "D:20260710000000");
        assert_eq!(parse("D:2026071014").unwrap().to_string(), "D:20260710140000");
    }

    #[test]
    fn an_absent_offset_stays_absent() {
        // "If no UT information is specified, the relationship of the
        // specified time to UT shall be considered GMT" — which is not the
        // same as a file that said so explicitly.
        assert_eq!(parse("D:20260710143000").unwrap().utc_offset_minutes, None);
        assert_eq!(parse("D:20260710143000").unwrap().to_string(), "D:20260710143000");
    }

    #[test]
    fn a_z_suffix_reads_as_zero() {
        let date = parse("D:20260710143000Z").unwrap();
        assert_eq!(date.utc_offset_minutes, Some(0));
        // The same instant, spelled the way the offset form spells it.
        assert_eq!(date.to_string(), "D:20260710143000+00'00'");
    }

    #[test]
    fn a_malformed_date_is_rejected_rather_than_guessed() {
        for bad in [
            "D:20x6",                // not digits where the year is due
            "D:",                    // nothing at all
            "D:20261510",            // month 15
            "D:20260732",            // day 32
            "D:20260710250000",      // hour 25
            "D:20260710146000",      // minute 60
            "D:20260710143099",      // second 99
            "D:20260710143000+2500", // offset hour 25
            "D:20260710143000*0200", // not a relation
        ] {
            assert!(parse(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn the_prefix_is_not_required_because_datetime_string_filters_it() {
        // `Object::datetime_string` removes every `D`, `:` and `'` wherever
        // they appear rather than anchoring a `D:` prefix, so a date written
        // without one has always been accepted here. Pinned so the leniency
        // is a decision rather than a surprise.
        assert_eq!(parse("20260710").unwrap().to_string(), "D:20260710000000");
    }

    #[test]
    fn the_object_round_trips() {
        let original = PdfDateTime {
            year: 1999,
            month: 12,
            day: 31,
            hour: 23,
            minute: 59,
            second: 59,
            utc_offset_minutes: Some(-45),
        };
        let object: Object = original.into();
        let back: PdfDateTime = object.as_datetime().unwrap().try_into().unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn a_non_string_object_is_not_a_datetime() {
        assert!(Object::Integer(20260710).as_datetime().is_none());
    }
}

#[cfg(feature = "chrono")]
#[test]
fn parse_datetime_local() {
    use chrono::prelude::*;

    let time = Local::now().with_nanosecond(0).unwrap();
    let text: Object = time.into();
    let time2: Option<DateTime<Local>> = text.as_datetime().and_then(|dt| dt.try_into().ok());
    assert_eq!(time2, Some(time));
}

#[cfg(feature = "chrono")]
#[test]
fn parse_datetime_utc() {
    use chrono::prelude::*;

    let time = Utc::now().with_nanosecond(0).unwrap();
    let text: Object = time.into();
    let time2: Option<DateTime<Local>> = text.as_datetime().and_then(|dt| dt.try_into().ok());
    assert_eq!(time2, Some(time.with_timezone(&Local)));
}

#[cfg(feature = "jiff")]
#[test]
fn parse_zoned() {
    use jiff::Zoned;

    let time = Zoned::now().with().subsec_nanosecond(0).build().unwrap();
    let text: Object = time.clone().into();
    let time2: Option<Zoned> = text.as_datetime().and_then(|dt| dt.try_into().ok());
    assert_eq!(time2, Some(time));
}

#[cfg(feature = "jiff")]
#[test]
fn parse_timestamp() {
    use jiff::Zoned;

    let time = Zoned::now().with().subsec_nanosecond(0).build().unwrap();
    let text: Object = time.timestamp().into();
    let time2: Option<Zoned> = text.as_datetime().and_then(|dt| dt.try_into().ok());
    assert_eq!(time2, Some(time));
}

#[cfg(feature = "chrono")]
#[test]
fn parse_datetime_seconds_missing_chrono() {
    use chrono::prelude::*;

    // this is the example from the PDF reference, version 1.7, chapter 3.8.3
    let text = Object::string_literal("D:199812231952-08'00'");
    let dt: Option<DateTime<Local>> = text.as_datetime().and_then(|dt| dt.try_into().ok());
    assert!(dt.is_some());
}

#[cfg(feature = "chrono")]
#[test]
fn parse_datetime_time_missing_chrono() {
    use chrono::prelude::*;

    let text = Object::string_literal("D:20040229");
    let dt: Option<DateTime<Local>> = text.as_datetime().and_then(|dt| dt.try_into().ok());
    assert!(dt.is_some());
}

#[cfg(feature = "jiff")]
#[test]
fn parse_datetime_seconds_missing_jiff() {
    use jiff::Zoned;

    // this is the example from the PDF reference, version 1.7, chapter 3.8.3
    let text = Object::string_literal("D:199812231952-08'00'");
    let dt: Option<Zoned> = text.as_datetime().and_then(|dt| dt.try_into().ok());
    assert!(dt.is_some());
}

#[cfg(feature = "jiff")]
#[test]
fn parse_datetime_time_missing_jiff() {
    use jiff::Zoned;

    let text = Object::string_literal("D:20040229");
    let dt: Option<Zoned> = text.as_datetime().and_then(|dt| dt.try_into().ok());
    assert!(dt.is_some());
}

#[cfg(feature = "time")]
#[test]
fn format_primitive_datetime_as_utc() {
    use time::{Date, Month, PrimitiveDateTime, Time};

    // The example date from the PDF reference (1.7, §3.8.3), as a naive
    // datetime: it must serialize with the year first and the `Z` suffix.
    let date = Date::from_calendar_date(1998, Month::December, 23).unwrap();
    let time = Time::from_hms(19, 52, 0).unwrap();
    let text: Object = PrimitiveDateTime::new(date, time).into();
    match &text {
        Object::String(bytes, _) => assert_eq!(bytes.as_slice(), b"D:19981223195200Z"),
        other => panic!("expected a string literal, got {other:?}"),
    }
}

#[cfg(feature = "time")]
#[test]
fn parse_datetime() {
    use time::OffsetDateTime;

    let time = OffsetDateTime::now_utc();

    let text: Object = time.into();
    let time2: OffsetDateTime = text.as_datetime().unwrap().try_into().unwrap();

    assert_eq!(time2.date(), time.date());

    // Ignore nanoseconds
    // - not important in the date parsing
    assert_eq!(time2.time().hour(), time.time().hour());
    assert_eq!(time2.time().minute(), time.time().minute());
    assert_eq!(time2.time().second(), time.time().second());
}
