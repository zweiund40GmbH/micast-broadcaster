mod parser;

use chrono::prelude::*;

#[derive(Debug)]
pub struct Scheduler {
    spots: Vec<Spot>,
}

#[derive(Debug)]
struct Spot {
    uri: String,
    runs_at: Vec<DateTime<Local>>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ScheduledSpot<'a> {
    pub uri: &'a str,
    pub runs_at: DateTime<Local>,
}

impl Scheduler {

    /// Create a new Spot list for a specific date from a file
    /// 
    /// should return a Result with all Spots for this given daten and all intervals
    pub fn from_file(path: &str, for_date: DateTime<Local>) -> Result<Scheduler, anyhow::Error> {
        let parsed_spots = parser::from_file(path)?;

        let mut scheduler = Scheduler {
            spots: Vec::new(),
        };

        scheduler.process(parsed_spots, for_date)?;

        Ok(scheduler)
    }

    /// Create a new Spot list from a str
    /// 
    /// should return a Result with all Spots for this given daten and all intervals
    pub fn from_str(data: &str, for_date: DateTime<Local>) -> Result<Scheduler, anyhow::Error> {
        let parsed_spots = parser::from_str(data)?;

        let mut scheduler = Scheduler {
            spots: Vec::new(),
        };

        scheduler.process(parsed_spots, for_date)?;

        Ok(scheduler)
    }

    /// ##process
    /// 
    /// - processing parsed spots.
    /// - filter out all spots outside of given for_date date
    /// - generate all intervals
    pub fn process(&mut self, spots: parser::TimeTable, for_date: DateTime<Local>) -> Result<(), anyhow::Error> {
        // get all valid spots (valid means they should allowed and activated for this day)
        // look at 'is_valid' in parser::Spot struct
        let spots:Vec<parser::Spot> = spots.spots.into_iter().filter(|spot| spot.is_valid(for_date)).collect();

        for spot in spots {
            let schedules:Vec<DateTime<Local>> = spot.schedules.iter().
                filter(|schedule| schedule.is_valid(for_date)).
                map(|schedule| {
                    schedule.generate_intervals(for_date).unwrap_or(Vec::new()).into_iter()
                }).flatten().collect::<Vec<DateTime<Local>>>();
            if schedules.len() > 0 {
                self.spots.push(Spot {
                    uri: spot.uri,
                    runs_at: schedules,
                });
            }
        }

        Ok(())

    
    }

    /// sort
    /// 
    /// create a sorted spot list based on current data
    pub fn sort(&self) -> Vec<ScheduledSpot> {

        let mut unsorted_list: Vec<ScheduledSpot> = self.spots.iter().map(|spot| {
            spot.runs_at.iter().map(|at| {
                ScheduledSpot { uri: &spot.uri, runs_at: *at }
            }).collect::<Vec<ScheduledSpot>>()
        }).flatten().collect::<Vec<ScheduledSpot>>();

        unsorted_list.sort_by(|a, b| a.runs_at.partial_cmp(&b.runs_at).unwrap());

        return unsorted_list;
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spots_load() {
        // 2022-04-04 10:30:00 ist ein Montag
        let s = r#"<TimeTable>
            <spots uri="file:///test.mp3" start="2022-04-01T17:00:00" end="2022-06-01T23:59:00">
                <schedules start="07:00" end="22:00" weekdays="Mon" interval="2h"/>
            </spots>
        </TimeTable>"#;

        let out = Scheduler::from_str(&s, Local.ymd(2022, 4, 4).and_hms(10, 30, 00)).unwrap();

        assert_eq!(out.spots[0].runs_at, vec!(
            Local.ymd(2022, 4, 4).and_hms(11, 00, 00),
            Local.ymd(2022, 4, 4).and_hms(13, 00, 00),
            Local.ymd(2022, 4, 4).and_hms(15, 00, 00),
            Local.ymd(2022, 4, 4).and_hms(17, 00, 00),
            Local.ymd(2022, 4, 4).and_hms(19, 00, 00),
            Local.ymd(2022, 4, 4).and_hms(21, 00, 00),
        ));

        // 2022-04-04 10:30:00 ist ein Montag
        let s = r#"<TimeTable>
            <spots uri="file:///test.mp3" start="2022-04-01T17:00:00" end="2022-06-01T23:59:00">
                <schedules start="09:20" end="22:00" weekdays="Sun-Mon,Fri" interval="4h"/>
            </spots>
        </TimeTable>"#;

        let out = Scheduler::from_str(&s, Local.ymd(2022, 4, 4).and_hms(10, 30, 00)).unwrap();

        assert_eq!(out.spots[0].runs_at, vec!(
            Local.ymd(2022, 4, 4).and_hms(13, 20, 00),
            Local.ymd(2022, 4, 4).and_hms(17, 20, 00),
            Local.ymd(2022, 4, 4).and_hms(21, 20, 00),
        ));

        let s = r#"<TimeTable>
            <spots uri="file:///test.mp3" start="2022-04-01T17:00:00" end="2022-06-01T23:59:00">
                <schedules start="07:00" end="22:00" weekdays="Mon" interval="2h"/>
                <schedules start="09:20" end="22:00" weekdays="Sun-Mon,Fri" interval="4h"/>
            </spots>
        </TimeTable>"#;

        let out = Scheduler::from_str(&s, Local.ymd(2022, 4, 4).and_hms(10, 30, 00)).unwrap();

        assert_eq!(out.spots[0].runs_at, vec!(
            Local.ymd(2022, 4, 4).and_hms(11, 00, 00),
            Local.ymd(2022, 4, 4).and_hms(13, 00, 00),
            Local.ymd(2022, 4, 4).and_hms(15, 00, 00),
            Local.ymd(2022, 4, 4).and_hms(17, 00, 00),
            Local.ymd(2022, 4, 4).and_hms(19, 00, 00),
            Local.ymd(2022, 4, 4).and_hms(21, 00, 00),
            Local.ymd(2022, 4, 4).and_hms(13, 20, 00),
            Local.ymd(2022, 4, 4).and_hms(17, 20, 00),
            Local.ymd(2022, 4, 4).and_hms(21, 20, 00),
        ));
    }

    #[test]
    fn spots_sort() {

        let s = r#"<TimeTable>
            <spots uri="file:///test.mp3" start="2022-04-01T17:00:00" end="2022-06-01T23:59:00">
                <schedules start="07:00" end="22:00" weekdays="Mon" interval="2h"/>
            </spots>
            <spots uri="file:///ab_9_2.mp3" start="2022-04-01T17:00:00" end="2022-06-01T23:59:00">
                <schedules start="09:20" end="22:00" weekdays="Sun-Mon,Fri" interval="4h"/>
            </spots>
        </TimeTable>"#;

        let out = Scheduler::from_str(&s, Local.ymd(2022, 4, 4).and_hms(10, 30, 00)).unwrap();

        let sorted_output = out.sort();

        assert_eq!(sorted_output, vec!(
            ScheduledSpot { uri: "file:///test.mp3", runs_at: Local.ymd(2022, 4, 4).and_hms(11, 00, 00) },
            ScheduledSpot { uri: "file:///test.mp3", runs_at: Local.ymd(2022, 4, 4).and_hms(13, 00, 00) },
            ScheduledSpot { uri: "file:///ab_9_2.mp3", runs_at: Local.ymd(2022, 4, 4).and_hms(13, 20, 00) },
            ScheduledSpot { uri: "file:///test.mp3", runs_at: Local.ymd(2022, 4, 4).and_hms(15, 00, 00) },
            ScheduledSpot { uri: "file:///test.mp3", runs_at: Local.ymd(2022, 4, 4).and_hms(17, 00, 00) },
            ScheduledSpot { uri: "file:///ab_9_2.mp3", runs_at: Local.ymd(2022, 4, 4).and_hms(17, 20, 00) },
            ScheduledSpot { uri: "file:///test.mp3", runs_at: Local.ymd(2022, 4, 4).and_hms(19, 00, 00) },
            ScheduledSpot { uri: "file:///test.mp3", runs_at: Local.ymd(2022, 4, 4).and_hms(21, 00, 00) },
            ScheduledSpot { uri: "file:///ab_9_2.mp3", runs_at: Local.ymd(2022, 4, 4).and_hms(21, 20, 00) },
            
        ));

    }
}