// Rust-oracle - Rust binding for Oracle database
//
// URL: https://github.com/kubo/rust-oracle
//
// ------------------------------------------------------
//
// Copyright 2017-2018 Kubo Takehiro <kubo@jiubao.org>
//
// Redistribution and use in source and binary forms, with or without modification, are
// permitted provided that the following conditions are met:
//
//    1. Redistributions of source code must retain the above copyright notice, this list of
//       conditions and the following disclaimer.
//
//    2. Redistributions in binary form must reproduce the above copyright notice, this list
//       of conditions and the following disclaimer in the documentation and/or other materials
//       provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHORS ''AS IS'' AND ANY EXPRESS OR IMPLIED
// WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND
// FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL <COPYRIGHT HOLDER> OR
// CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON
// ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
// NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF
// ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//
// The views and conclusions contained in the software and documentation are those of the
// authors and should not be interpreted as representing official policies, either expressed
// or implied, of the authors.

use std::cmp;
use std::fmt;
use std::str;

use binding::dpiIntervalDS;
use util::Scanner;
use OracleType;
use ParseOracleTypeError;

/// Oracle-specific [Interval Day to Second][INTVL_DS] data type.
///
/// [INTVL_DS]: https://docs.oracle.com/database/122/NLSPG/datetime-data-types-and-time-zone-support.htm#GUID-FD8C41B7-8CDC-4D02-8E6B-5250416BC17D
///
/// This struct doesn't have arithmetic methods and they won't be added to avoid
/// reinventing the wheel. If you need methods such as adding an interval to a
/// timestamp, enable `chrono` feature and use [chrono::Duration][] instead.
///
/// [chrono::Duration]: https://docs.rs/chrono/0.4/chrono/struct.Duration.html
///
/// # Examples
///
/// ```
/// use oracle::IntervalDS;
///
/// // Create an interval by new().
/// let intvl1 = IntervalDS::new(1, 2, 3, 4, 500000000);
///
/// // All arguments must be zero or negative to create a negative interval.
/// let intvl2 = IntervalDS::new(-1, -2, -3, -4, -500000000);
///
/// // Convert to string.
/// assert_eq!(intvl1.to_string(), "+000000001 02:03:04.500000000");
/// assert_eq!(intvl2.to_string(), "-000000001 02:03:04.500000000");
///
/// // Create an interval with leading field and fractional second precisions.
/// let intvl3 = IntervalDS::new(1, 2, 3, 4, 500000000).and_prec(2, 3);
///
/// // The string representation depends on the precisions.
/// assert_eq!(intvl3.to_string(), "+01 02:03:04.500");
///
/// // Precisions are ignored when intervals are compared.
/// assert!(intvl1 == intvl3);
///
/// // Create an interval from string.
/// let intvl4: IntervalDS = "+1 02:03:04.50".parse().unwrap();
///
/// // The precisions are determined by number of decimal digits in the string.
/// assert_eq!(intvl4.lfprec(), 1);
/// assert_eq!(intvl4.fsprec(), 2);
/// ```
///
/// Fetch and bind interval values.
///
/// ```
/// use oracle::{Connection, IntervalDS, OracleType, Timestamp};
///
/// let conn = Connection::new("scott", "tiger", "").unwrap();
///
/// // Fetch IntervalDS
/// let sql = "select interval '+01 02:03:04.500' day to second(3) from dual";
/// let mut stmt = conn.execute(sql, &[]).unwrap();
/// let row = stmt.fetch().unwrap();
/// let intvl: IntervalDS = row.get(0).unwrap();
/// assert_eq!(intvl.to_string(), "+01 02:03:04.500");
///
/// // Bind IntervalDS
/// let sql = "begin \
///              :outval := to_timestamp('2017-08-09', 'yyyy-mm-dd') + :inval; \
///            end;";
/// let stmt = conn.execute(sql,
///                         &[&OracleType::Timestamp(3), // bind null as timestamp(3)
///                           &intvl, // bind the intvl variable
///                          ]).unwrap();
/// let outval: Timestamp = stmt.bind_value(1).unwrap(); // get the first bind value.
/// // 2017-08-09 + (1 day, 2 hours, 3 minutes and 4.5 seconds)
/// assert_eq!(outval.to_string(), "2017-08-10 02:03:04.500");
/// ```
#[derive(Debug, Clone, Copy)]
pub struct IntervalDS {
    days: i32,
    hours: i32,
    minutes: i32,
    seconds: i32,
    nanoseconds: i32,
    lfprec: u8,
    fsprec: u8,
}

impl IntervalDS {
    pub(crate) fn from_dpi_interval_ds(it: &dpiIntervalDS, oratype: &OracleType) -> IntervalDS {
        let (lfprec, fsprec) = match *oratype {
            OracleType::IntervalDS(lfprec, fsprec) => (lfprec, fsprec),
            _ => (0, 0),
        };
        IntervalDS {
            days: it.days,
            hours: it.hours,
            minutes: it.minutes,
            seconds: it.seconds,
            nanoseconds: it.fseconds,
            lfprec: lfprec,
            fsprec: fsprec,
        }
    }

    /// Creates a new IntervalDS.
    ///
    /// Valid values are:
    ///
    /// | argument | valid values |
    /// |---|---|
    /// | `days` | -999999999 to 999999999 |
    /// | `hours` | -23 to 23 |
    /// | `minutes` | -59 to 59 |
    /// | `seconds` | -59 to 59 |
    /// | `nanoseconds` | -999999999 to 999999999 |
    ///
    /// All arguments must be zero or positive to create a positive interval.
    /// All arguments must be zero or negative to create a negative interval.
    pub fn new(days: i32, hours: i32, minutes: i32, seconds: i32, nanoseconds: i32) -> IntervalDS {
        IntervalDS {
            days: days,
            hours: hours,
            minutes: minutes,
            seconds: seconds,
            nanoseconds: nanoseconds,
            lfprec: 9,
            fsprec: 9,
        }
    }

    /// Creates a new IntervalDS with precisions.
    ///
    /// `lfprec` and `fsprec` are leading field precision and fractional second
    /// precision respectively.
    /// The precisions affect text representation of IntervalDS.
    /// They don't affect comparison.
    pub fn and_prec(&self, lfprec: u8, fsprec: u8) -> IntervalDS {
        IntervalDS {
            lfprec: lfprec,
            fsprec: fsprec,
            .. *self
        }
    }

    /// Returns days component.
    pub fn days(&self) -> i32 {
        self.days
    }

    /// Returns hours component.
    pub fn hours(&self) -> i32 {
        self.hours
    }

    /// Returns minutes component.
    pub fn minutes(&self) -> i32 {
        self.minutes
    }

    /// Returns seconds component.
    pub fn seconds(&self) -> i32 {
        self.seconds
    }

    /// Returns nanoseconds component.
    pub fn nanoseconds(&self) -> i32 {
        self.nanoseconds
    }

    /// Returns leading field precision.
    pub fn lfprec(&self) -> u8 {
        self.lfprec
    }

    /// Returns fractional second precision.
    pub fn fsprec(&self) -> u8 {
        self.fsprec
    }
}

impl cmp::PartialEq for IntervalDS {
    fn eq(&self, other: &Self) -> bool {
        self.days == other.days
            && self.hours == other.hours
            && self.minutes == other.minutes
            && self.seconds == other.seconds
            && self.nanoseconds == other.nanoseconds
    }
}

impl fmt::Display for IntervalDS {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.days < 0 || self.hours < 0 || self.minutes < 0 || self.seconds < 0 || self.nanoseconds < 0 {
            write!(f, "-")?;
        } else {
            write!(f, "+")?;
        };
        let days = self.days.abs();
        match self.lfprec {
            2 => write!(f, "{:02}", days)?,
            3 => write!(f, "{:03}", days)?,
            4 => write!(f, "{:04}", days)?,
            5 => write!(f, "{:05}", days)?,
            6 => write!(f, "{:06}", days)?,
            7 => write!(f, "{:07}", days)?,
            8 => write!(f, "{:08}", days)?,
            9 => write!(f, "{:09}", days)?,
            _ => write!(f, "{}", days)?,
        };
        write!(f, " {:02}:{:02}:{:02}", self.hours.abs(), self.minutes.abs(), self.seconds.abs())?;
        let nsec = self.nanoseconds.abs();
        match self.fsprec {
            1 => write!(f, ".{:01}", nsec / 100000000),
            2 => write!(f, ".{:02}", nsec / 10000000),
            3 => write!(f, ".{:03}", nsec / 1000000),
            4 => write!(f, ".{:04}", nsec / 100000),
            5 => write!(f, ".{:05}", nsec / 10000),
            6 => write!(f, ".{:06}", nsec / 1000),
            7 => write!(f, ".{:07}", nsec / 100),
            8 => write!(f, ".{:08}", nsec / 10),
            9 => write!(f, ".{:09}", nsec),
            _ => Ok(()),
        }
    }
}

impl str::FromStr for IntervalDS {
    type Err = ParseOracleTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseOracleTypeError::new("IntervalDS");
        let mut s = Scanner::new(s);
        let minus = match s.char() {
            Some('+') => {
                s.next();
                false
            },
            Some('-') => {
                s.next();
                true
            },
            _ => false,
        };
        let days = s.read_digits().ok_or(err())? as i32;
        let lfprec = s.ndigits();
        if let Some(' ') = s.char() {
            s.next();
        } else {
            return Err(err());
        }
        let hours = s.read_digits().ok_or(err())? as i32;
        if let Some(':') = s.char() {
            s.next();
        } else {
            return Err(err());
        }
        let minutes = s.read_digits().ok_or(err())? as i32;
        if let Some(':') = s.char() {
            s.next();
        } else {
            return Err(err());
        }
        let seconds = s.read_digits().ok_or(err())? as i32;
        let mut nsecs = 0;
        let mut fsprec = 0;
        if let Some('.') = s.char() {
            s.next();
            nsecs = s.read_digits().ok_or(err())? as i32;
            let ndigit = s.ndigits();
            fsprec = ndigit;
            if ndigit < 9 {
                nsecs *= 10i32.pow(9 - ndigit);
            } else if ndigit > 9 {
                nsecs /= 10i32.pow(ndigit - 9);
                fsprec = 9;
            }
        }
        if s.char().is_some() {
            return Err(err())
        }
        Ok(IntervalDS {
            days: if minus { -days } else { days },
            hours: if minus { -hours } else { hours },
            minutes: if minus { -minutes } else { minutes },
            seconds: if minus { -seconds } else { seconds },
            nanoseconds: if minus { -nsecs } else { nsecs },
            lfprec: lfprec as u8,
            fsprec: fsprec as u8,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_string() {
        let mut it = IntervalDS::new(1, 2, 3, 4, 123456789);
        it.lfprec = 0; it.fsprec = 0;
        assert_eq!(it.to_string(), "+1 02:03:04");
        it.fsprec = 1;
        assert_eq!(it.to_string(), "+1 02:03:04.1");
        it.fsprec = 2;
        assert_eq!(it.to_string(), "+1 02:03:04.12");
        it.fsprec = 3;
        assert_eq!(it.to_string(), "+1 02:03:04.123");
        it.fsprec = 4;
        assert_eq!(it.to_string(), "+1 02:03:04.1234");
        it.fsprec = 5;
        assert_eq!(it.to_string(), "+1 02:03:04.12345");
        it.fsprec = 6;
        assert_eq!(it.to_string(), "+1 02:03:04.123456");
        it.fsprec = 7;
        assert_eq!(it.to_string(), "+1 02:03:04.1234567");
        it.fsprec = 8;
        assert_eq!(it.to_string(), "+1 02:03:04.12345678");
        it.fsprec = 9;
        assert_eq!(it.to_string(), "+1 02:03:04.123456789");

        let mut it = IntervalDS::new(-1, -2, -3, -4, -123456789);
        it.lfprec = 0; it.fsprec = 0;
        assert_eq!(it.to_string(), "-1 02:03:04");
        it.fsprec = 1;
        assert_eq!(it.to_string(), "-1 02:03:04.1");
        it.fsprec = 2;
        assert_eq!(it.to_string(), "-1 02:03:04.12");
        it.fsprec = 3;
        assert_eq!(it.to_string(), "-1 02:03:04.123");
        it.fsprec = 4;
        assert_eq!(it.to_string(), "-1 02:03:04.1234");
        it.fsprec = 5;
        assert_eq!(it.to_string(), "-1 02:03:04.12345");
        it.fsprec = 6;
        assert_eq!(it.to_string(), "-1 02:03:04.123456");
        it.fsprec = 7;
        assert_eq!(it.to_string(), "-1 02:03:04.1234567");
        it.fsprec = 8;
        assert_eq!(it.to_string(), "-1 02:03:04.12345678");
        it.fsprec = 9;
        assert_eq!(it.to_string(), "-1 02:03:04.123456789");

        it.lfprec = 1;
        assert_eq!(it.to_string(), "-1 02:03:04.123456789");
        it.lfprec = 2;
        assert_eq!(it.to_string(), "-01 02:03:04.123456789");
        it.lfprec = 3;
        assert_eq!(it.to_string(), "-001 02:03:04.123456789");
        it.lfprec = 4;
        assert_eq!(it.to_string(), "-0001 02:03:04.123456789");
        it.lfprec = 5;
        assert_eq!(it.to_string(), "-00001 02:03:04.123456789");
        it.lfprec = 6;
        assert_eq!(it.to_string(), "-000001 02:03:04.123456789");
        it.lfprec = 7;
        assert_eq!(it.to_string(), "-0000001 02:03:04.123456789");
        it.lfprec = 8;
        assert_eq!(it.to_string(), "-00000001 02:03:04.123456789");
        it.lfprec = 9;
        assert_eq!(it.to_string(), "-000000001 02:03:04.123456789");
    }

    #[test]
    fn parse() {
        let mut it = IntervalDS::new(1, 2, 3, 4, 0);
        it.lfprec = 1; it.fsprec = 0;
        assert_eq!("1 02:03:04".parse(), Ok(it));
        assert_eq!("+1 02:03:04".parse(), Ok(it));
        it.lfprec = 2;
        assert_eq!("01 02:03:04".parse(), Ok(it));
        it.lfprec = 3;
        assert_eq!("001 02:03:04".parse(), Ok(it));
        it.lfprec = 4;
        assert_eq!("0001 02:03:04".parse(), Ok(it));
        it.lfprec = 5;
        assert_eq!("00001 02:03:04".parse(), Ok(it));
        it.lfprec = 6;
        assert_eq!("000001 02:03:04".parse(), Ok(it));
        it.lfprec = 7;
        assert_eq!("0000001 02:03:04".parse(), Ok(it));
        it.lfprec = 8;
        assert_eq!("00000001 02:03:04".parse(), Ok(it));
        it.lfprec = 9;
        assert_eq!("000000001 02:03:04".parse(), Ok(it));

        it.fsprec = 1; it.nanoseconds = 100000000;
        assert_eq!("000000001 02:03:04.1".parse(), Ok(it));

        let mut it = IntervalDS::new(-1, -2, -3, -4, 0);
        it.lfprec = 1; it.fsprec = 0;
        assert_eq!("-1 02:03:04".parse(), Ok(it));

        it.fsprec = 1; it.nanoseconds = -100000000;
        assert_eq!("-1 02:03:04.1".parse(), Ok(it));
        it.fsprec = 2; it.nanoseconds = -120000000;
        assert_eq!("-1 02:03:04.12".parse(), Ok(it));
        it.fsprec = 3; it.nanoseconds = -123000000;
        assert_eq!("-1 02:03:04.123".parse(), Ok(it));
        it.fsprec = 4; it.nanoseconds = -123400000;
        assert_eq!("-1 02:03:04.1234".parse(), Ok(it));
        it.fsprec = 5; it.nanoseconds = -123450000;
        assert_eq!("-1 02:03:04.12345".parse(), Ok(it));
        it.fsprec = 6; it.nanoseconds = -123456000;
        assert_eq!("-1 02:03:04.123456".parse(), Ok(it));
        it.fsprec = 7; it.nanoseconds = -123456700;
        assert_eq!("-1 02:03:04.1234567".parse(), Ok(it));
        it.fsprec = 8; it.nanoseconds = -123456780;
        assert_eq!("-1 02:03:04.12345678".parse(), Ok(it));
        it.fsprec = 9; it.nanoseconds = -123456789;
        assert_eq!("-1 02:03:04.123456789".parse(), Ok(it));
    }
}
