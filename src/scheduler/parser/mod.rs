
mod custom_serde;

use serde::{Deserialize, Serialize};

use quick_xml::de::{from_reader};
use chrono::prelude::*;
use chrono::Duration;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct TimeTable {
    pub(crate) spots: Vec<Spot>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Spot {
    pub uri: String,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub schedules: Vec<Schedule>,
}

impl Spot {
    //check if current spot is in a valid range between start and end
    pub(crate) fn is_valid(&self, for_date: DateTime<Local>) -> bool {

        let start = Local.from_local_datetime(&self.start).unwrap();
        let end = Local.from_local_datetime(&self.end).unwrap();
        //let start = DateTime::<Local>::from_utc(self.start, Local);

        if start <= for_date && end >= for_date {
            return true
        } 
        false
    }
}


#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Schedule {
    #[serde(with = "custom_serde::hourminutes")]
    pub start: Option<(u16,u16)>,

    #[serde(default)]
    #[serde(with = "custom_serde::hourminutes")]
    pub end: Option<(u16,u16)>,

    #[serde(with = "custom_serde::weekaler")]
    pub weekdays: Vec<Weekday>,

    #[serde(default)]
    pub interval: String,
}



impl Schedule {
    //check if current spot is in a valid range between start and end
    pub fn is_valid(&self, for_date: DateTime<Local>) -> bool {
        let now = for_date;
        let current_weekday = now.weekday();
        let current_hour = now.hour() as u16;
        let current_minute = now.minute() as u16;

        if self.weekdays.contains(&current_weekday) {

            if self.start.is_some() {
                
                //if current_hour > start.0 || (start.0 == current_hour && current_minute >= start.1) {
                if let Some(end) = self.end {
                    if current_hour > end.0 || (current_hour == end.0 && current_minute >= end.1) {
                        return false;
                    } 
                }
                
                
            }

            return true;
        }

        false
    }

    /// generate a list of intervals for in 'for_date' specific date
    /// 
    /// should return non if for this day nothing valid intervals are found
    pub fn generate_intervals(&self, for_date: DateTime<Local>) -> Option<Vec<DateTime<Local>>> {

        let start_point = self.start.unwrap_or((0,0));
        let mut now = for_date.date().and_hms(start_point.0 as u32,start_point.1 as u32,0);
        let today = now.day();

        let end_point = {
            let a = self.end.unwrap_or((23,59));
            (a.0 as u32, a.1 as u32)
        };
        
        let mut generated_intervals: Vec<DateTime<Local>> = Vec::new();

        

        let (interval, type_of_interval) = {
            let mut interval_string = self.interval.to_string();
            let type_of_interval = interval_string.pop().unwrap_or('h');
            let unparsed_int = interval_string.as_str();

            (unparsed_int.parse::<i64>().unwrap_or(0), type_of_interval)
        };

        if interval == 0 {
            generated_intervals.push(now);
            return Some(generated_intervals)
        }

        // be sure we generate no intervals after the end_point
        while today == now.day() && (now.hour() < end_point.0 || (now.hour() == end_point.0 && now.minute() <= end_point.0))  {
            generated_intervals.push(now);

            match type_of_interval {
                'm' => {
                    now = now + Duration::minutes(interval);
                },
                'h' => {
                    now = now + Duration::hours(interval);
                },
                _ => {
                    panic!("das darf nicht passieren")
                }
            };

            
            
        }

        // remove all intervals from list which are older than current timestamp
        generated_intervals.retain(|interval| *interval >= for_date);

        //generated_intervals.pop();

        Some(generated_intervals)
    }
}


//load_spots loading a xml file with spots and the scheduling
pub fn from_file(path: &str) -> Result<TimeTable,anyhow::Error> {
    use std::io::BufReader;
    use std::fs::File;

    let f = File::open(path)?;
    let f = BufReader::new(f);

    let spots: TimeTable = from_reader(f)?;

    Ok(spots)
}

//load_spots loading a xml string with spots and the scheduling
pub fn from_str(data: &str) -> Result<TimeTable,anyhow::Error> {

    let spots: TimeTable = quick_xml::de::from_str(data)?;

    Ok(spots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_intervals() {
        let s =Schedule {
            start: Some((7,33).into()),
            end: Some((22,15).into()),
            weekdays: vec![Weekday::Mon, Weekday::Tue, Weekday::Fri],
            interval: "40m".to_string(),
        };

        let intis = s.generate_intervals(Local.ymd(2022, 6, 1).and_hms(8, 59, 00));
        let r = intis.unwrap_or(Vec::new());
        println!("generated intervals: {:?} len:{}", &r, &r.len());
    }


    #[test]
    fn generate_intervals_with_no_interval_settings() {
        let s =Schedule {
            start: Some((7,33).into()),
            end: Some((22,15).into()),
            weekdays: vec![Weekday::Mon, Weekday::Tue, Weekday::Fri],
            interval: "".to_string(),
        };

        let out = s.generate_intervals(Local.ymd(2022, 6, 1).and_hms(8, 59, 00)).unwrap();

        assert_eq!(out.len(), 1);
        assert_eq!(out[0], Local.ymd(2022, 6, 1).and_hms(7,33,0));


        let s =Schedule {
            start: Some((7,33).into()),
            end: None,
            weekdays: vec![Weekday::Mon, Weekday::Tue, Weekday::Fri],
            interval: "".to_string(),
        };

        let out = s.generate_intervals(Local.ymd(2022, 6, 1).and_hms(8, 59, 00)).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], Local.ymd(2022, 6, 1).and_hms(7,33,0));

        let s =Schedule {
            start: Some((7,33).into()),
            end: None,
            weekdays: Vec::new(),
            interval: "".to_string(),
        };

        let out = s.generate_intervals(Local.ymd(2022, 6, 1).and_hms(8, 59, 00)).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], Local.ymd(2022, 6, 1).and_hms(7,33,0));
        
    }

    #[test]
    fn scheduler_xml_to_string() {
        let s = TimeTable {
            spots: vec![
                Spot { 
                    uri: "file:///test.mp3".to_string(),
                    start: NaiveDate::from_ymd(2022, 4, 1).and_hms(17, 00, 00),
                    end: NaiveDate::from_ymd(2022, 6, 1).and_hms(23, 59, 00),
                    schedules: vec![
                        Schedule {
                            start: Some((7,33).into()),
                            end: Some((22,15).into()),
                            weekdays: vec![Weekday::Mon, Weekday::Tue, Weekday::Fri],
                            interval: "2h".to_string(),
                        },
                        Schedule {
                            start: Some((10,0).into()),
                            end: Some((22,0).into()),
                            weekdays: vec![Weekday::Sun],
                            interval: "2h".to_string(),
                        },
                        Schedule {
                            start: Some((10,0).into()),
                            end: Some((22,0).into()),
                            weekdays: vec![Weekday::Sun, Weekday::Mon, Weekday::Fri],
                            interval: "2h".to_string(),
                        }
                    ]
                }
            ]
        };

        let out = quick_xml::se::to_string(&s).unwrap();

        assert_eq!(out, r#"<TimeTable><spots uri="file:///test.mp3" start="2022-04-01T17:00:00" end="2022-06-01T23:59:00"><schedules start="07:33" end="22:15" weekdays="Mon-Tue,Fri" interval="2h"/><schedules start="10:00" end="22:00" weekdays="Sun" interval="2h"/><schedules start="10:00" end="22:00" weekdays="Sun-Mon,Fri" interval="2h"/></spots></TimeTable>"#);
    }

    #[test]
    fn scheduler_string_to_xml() {
        let s = r#"<TimeTable>
            <spots uri="file:///test.mp3" start="2022-04-01T17:00:00" end="2022-06-01T23:59:00">
                <schedules start="07:33" end="22:15" weekdays="Mon-Tue,Fri" interval="2h"/>
                <schedules start="10:00" end="22:00" weekdays="Sun" interval="2h"/>
                <schedules start="10:00" end="22:00" weekdays="Sun-Mon,Fri" interval="2h"/>
            </spots>
        </TimeTable>"#;

        let out: TimeTable = from_str(&s).unwrap();

        assert_eq!(out, TimeTable {
            spots: vec![
                Spot { 
                    uri: "file:///test.mp3".to_string(),
                    start: NaiveDate::from_ymd(2022, 4, 1).and_hms(17, 00, 00),
                    end: NaiveDate::from_ymd(2022, 6, 1).and_hms(23, 59, 00),
                    schedules: vec![
                        Schedule {
                            start: Some((7,33).into()),
                            end: Some((22,15).into()),
                            weekdays: vec![Weekday::Mon, Weekday::Tue, Weekday::Fri],
                            interval: "2h".to_string(),
                        },
                        Schedule {
                            start: Some((10,0).into()),
                            end: Some((22,0).into()),
                            weekdays: vec![Weekday::Sun],
                            interval: "2h".to_string(),
                        },
                        Schedule {
                            start: Some((10,0).into()),
                            end: Some((22,0).into()),
                            weekdays: vec![Weekday::Sun, Weekday::Mon, Weekday::Fri],
                            interval: "2h".to_string(),
                        }
                    ]
                }
            ]
        });
    }
}
