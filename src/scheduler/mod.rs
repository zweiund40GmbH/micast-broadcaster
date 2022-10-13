mod parser;
mod fetchfiles;

use anyhow::bail;
use chrono::prelude::*;
use log::{info, warn};


use std::sync::mpsc;

#[derive(Debug)]
pub struct Scheduler {
    spots: Vec<Spot>,
    files: Vec<parser::File>,
    load_files: bool,
    last_spot: Option<ScheduledSpot>,
    parsed_timetable: Option<parser::TimeTable>,
    recv: mpsc::Receiver<Vec<parser::File>>,
    sender: mpsc::Sender<Vec<parser::File>>,
}

#[derive(Debug, Clone)]
struct Spot {
    uri: String,
    runs_at: Vec<DateTime<Local>>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ScheduledSpot {
    pub uri: String,
    pub runs_at: DateTime<Local>,
}

impl Scheduler {

    pub fn new() -> Scheduler {
        
        let (sender, recv) = mpsc::channel();
        Scheduler {
            spots: Vec::new(),
            files: Vec::new(),
            load_files: false,
            last_spot: None,
            parsed_timetable: None,
            recv,
            sender,
        }
    }

    pub fn get_now() -> DateTime<Local> {
        Local::now()
    }

    /// Create a new Spot list for a specific date from a file
    /// 
    /// should return a Result with all Spots for this given daten and all intervals
    pub fn from_file(&mut self, path: &str) -> Result<(), anyhow::Error> {
        let parsed_timetable = parser::from_file(path)?;
        self.parsed_timetable = Some(parsed_timetable);

        Ok(())
    }

    /// Create a new Spot list from a str
    /// 
    /// should return a Result with all Spots for this given daten and all intervals
    pub fn from_str(&mut self, data: &str) -> Result<(), anyhow::Error> {
        let parsed_timetable = parser::from_str(data)?;
        self.parsed_timetable = Some(parsed_timetable);

        Ok(())
    }

    pub fn load_files(&mut self) -> Result<(), anyhow::Error> {

        // load the files from remote to local... then replace filenames and so on
        self.load_files = true;
        let timetable = self.parsed_timetable.as_ref().unwrap();
        fetchfiles::download_files(timetable.files.clone(), self.sender.clone());

        Ok(())
    }

    pub fn next(&mut self, for_date: DateTime<Local>) -> Result<ScheduledSpot, anyhow::Error> {

        self.process(for_date)?;

        let spots = self.sort();
        // window of next spot - 1 minute and next_spot + 1 minute

        for spot in spots {

            if let Some(last_spot) = &self.last_spot {
                if *last_spot == spot {
                    continue;
                }
            }

            if for_date > spot.runs_at + chrono::Duration::minutes(1) {
                // remove it
                continue;
            }

            if for_date >= spot.runs_at && for_date < spot.runs_at + chrono::Duration::minutes(1) {
                
                info!("found spot for playback (current date: {:?}) spot: {}, runs at {}", for_date, spot.uri, spot.runs_at);
                self.last_spot = Some(spot.clone());
                return Ok(spot);
                
            }
        }

        bail!("nothing");
    }

    /// ##process
    /// 
    /// - processing parsed spots.
    /// - filter out all spots outside of given for_date date
    /// - generate all intervals
    fn process(&mut self, for_date: DateTime<Local>) -> Result<(), anyhow::Error> {

        if self.load_files == true {
            if let Ok(files) = self.recv.try_recv() {
                info!("received some downloaded files, set files from downloaded paths");
                self.files = files;
            }
        } else {

            if self.files.len() == 0 {
                info!("no remote downloading enabled, use files without download");
                let timetable = self.parsed_timetable.as_ref().unwrap();
                self.files = timetable.files.clone();
            }
        }


        if self.files.len() == 0 {
            warn!("no files downloaded / found, skip proccessing scheduler");
            return Ok(())
        }

        // get all valid spots (valid means they should allowed and activated for this day)
        // look at 'is_valid' in parser::Spot struct
        //let for_date = for_date + chrono::Duration::hours(2);

        if self.parsed_timetable.is_none() {
            bail!("timetable not parsed!");
        }

        let timetable = self.parsed_timetable.as_ref().unwrap();

        let spots = timetable.spots.clone();

        let spots:Vec<parser::Spot> = spots.into_iter().filter(|spot| spot.is_valid(for_date)).collect();

        for spot in spots {
            let schedules:Vec<DateTime<Local>> = spot.schedules.iter().
                filter(|schedule| schedule.is_valid(for_date)).
                map(|schedule| {
                    schedule.generate_intervals(for_date).unwrap_or(Vec::new()).into_iter()
                }).flatten().collect::<Vec<DateTime<Local>>>();
            if schedules.len() > 0 {
                if let Some(local_uri) = self.files.iter().find(|file| file.id == spot.file).map(|file| format!("file://{}",file.local.as_ref().unwrap_or(&file.uri))) {
                    self.spots.push(Spot {
                        uri: local_uri,
                        runs_at: schedules,
                    });
                } else {
                    bail!("no local uri found");
                }
                
            }
        }

        Ok(())

    
    }

    /// sort
    /// 
    /// create a sorted spot list based on current data
    fn sort(&self) -> Vec<ScheduledSpot> {

        let mut unsorted_list: Vec<ScheduledSpot> = self.spots.iter().map(|spot| {
            spot.runs_at.iter().map(|at| {
                ScheduledSpot { uri: spot.uri.clone(), runs_at: *at }
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
    fn spots_next() {
        let s = r#"<TimeTable>
        <files id="0" uri="https://dev.micast.de/media.micast.de/spots/8487 - Big Cash Instore Spots DE 03-2019 Mix-03 - 01_09_15.wav"/>
        <spots file="0" start="2022-04-01T17:00:00" end="2022-06-01T23:59:00">
                <schedules start="08:00" end="22:00" weekdays="Mon" interval="5m"/>
            </spots>
        </TimeTable>"#;

        

        //println!("local_time: {}", local_time);
        let mut scheduler = Scheduler::new();
        let _ = scheduler.from_str(&s);
        
        let local_time = Local.ymd(2022, 4, 4).and_hms(08, 3, 00);

        let out = scheduler.next(local_time);

        assert!(out.is_err());

        let local_time = Local.ymd(2022, 4, 4).and_hms(08, 4, 00);
        let out = scheduler.next(local_time);
        assert!(out.is_err());

        let local_time = Local.ymd(2022, 4, 4).and_hms(08, 5, 00);
        let out = scheduler.next(local_time);
        assert!(out.is_ok());

        let local_time = Local.ymd(2022, 4, 4).and_hms(08, 5, 30);
        let out = scheduler.next(local_time);
        assert!(out.is_err());

        let local_time = Local.ymd(2022, 4, 4).and_hms(08, 5, 36);
        let out = scheduler.next(local_time);
        assert!(out.is_err());

        let local_time = Local.ymd(2022, 4, 4).and_hms(08, 9, 55);
        let out = scheduler.next(local_time);
        assert!(out.is_err());

        let local_time = Local.ymd(2022, 4, 4).and_hms(08, 10, 0);
        let out = scheduler.next(local_time);
        assert!(out.is_ok());

        let local_time = Local.ymd(2022, 4, 4).and_hms(22, 00, 0);
        let out = scheduler.next(local_time);
        assert!(out.is_ok());


        let local_time = Local.ymd(2022, 4, 4).and_hms(22, 05, 0);
        let out = scheduler.next(local_time);
        assert!(out.is_err());
    }


    #[test]
    fn spots_date_tests() {
        let s = r#"<TimeTable>
            <files id="0" uri="https://dev.micast.de/media.micast.de/spots/8487 - Big Cash Instore Spots DE 03-2019 Mix-03 - 01_09_15.wav"/>
            <spots file="0" start="2022-04-01T17:00:00" end="2022-06-01T23:59:00">
                <schedules start="10:30" end="22:00" weekdays="Mon" interval="2h"/>
            </spots>
        </TimeTable>"#;

        let local_time = Local.ymd(2022, 4, 4).and_hms(10, 29, 00);

        //println!("local_time: {}", local_time);
        let mut out = Scheduler::new();
        let _ = out.from_str(&s);
        let _ = out.process(local_time);


        println!("output: {:?}", out);

        assert!(out.spots.len() > 0);
    }

    #[test]
    fn spots_single_shot_every_day() {
        env_logger::init();

        info!("hello");

        let s = r#"<TimeTable>
            <files id="0" uri="https://dev.micast.de/media.micast.de/spots/8487 - Big Cash Instore Spots DE 03-2019 Mix-03 - 01_09_15.wav"/>
        
            <spots file="0" start="2022-04-01T17:00:00" end="2090-06-01T23:59:00">
                <schedules start="10:00" end="10:05" interval="2h" weekdays="Mon-Sun"/>
            </spots>
        </TimeTable>"#;

        let local_time = Local.ymd(2022, 4, 4).and_hms(10, 00, 00);

        //println!("local_time: {}", local_time);
        let mut out = Scheduler::new();
        let e = out.from_str(&s);
        if let Err(e) = e {
            warn!("error on parsing : {:#?}", e);
        }
        let _ = out.process(local_time);


        println!("output: {:?}", out);

        assert!(out.spots.len() == 1);
    }

    #[test]
    fn spots_date_tests_2() {
        let s = r#"<TimeTable>
        <files id="0" uri="https://dev.micast.de/media.micast.de/spots/8487 - Big Cash Instore Spots DE 03-2019 Mix-03 - 01_09_15.wav"/>
        <spots file="0" start="2022-04-01T17:00:00" end="2022-06-01T23:59:00">
                <schedules start="07:00" end="22:00" weekdays="Mon" interval="2h"/>
            </spots>
        </TimeTable>"#;

        let local_time = Local.ymd(2022, 4, 4).and_hms(10, 29, 00);

        //println!("local_time: {}", local_time);
        let mut out = Scheduler::new();
        let _ = out.from_str(&s);
        let _ = out.process(local_time);

        //println!("output: {:?}", out);

        assert_eq!(out.spots[0].runs_at.len(), 6);
    }

    #[test]
    fn spots_load() {
        // 2022-04-04 10:30:00 ist ein Montag
        let s = r#"<TimeTable>
        <files id="0" uri="https://dev.micast.de/media.micast.de/spots/8487 - Big Cash Instore Spots DE 03-2019 Mix-03 - 01_09_15.wav"/>
        <spots file="0" start="2022-04-01T17:00:00" end="2022-06-01T23:59:00">
                <schedules start="07:00" end="22:00" weekdays="Mon" interval="2h"/>
            </spots>
        </TimeTable>"#;

        let mut out = Scheduler::new();
        let _ = out.from_str(&s);
        let _ = out.process(Local.ymd(2022, 4, 4).and_hms(10, 30, 00));

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
        <files id="0" uri="https://dev.micast.de/media.micast.de/spots/8487 - Big Cash Instore Spots DE 03-2019 Mix-03 - 01_09_15.wav"/>
        <spots file="0" start="2022-04-01T17:00:00" end="2022-06-01T23:59:00">
                <schedules start="09:20" end="22:00" weekdays="Sun-Mon,Fri" interval="4h"/>
            </spots>
        </TimeTable>"#;

        let mut out = Scheduler::new();
        let _ = out.from_str(&s);
        let _ = out.process(Local.ymd(2022, 4, 4).and_hms(10, 30, 00));

        assert_eq!(out.spots[0].runs_at, vec!(
            Local.ymd(2022, 4, 4).and_hms(13, 20, 00),
            Local.ymd(2022, 4, 4).and_hms(17, 20, 00),
            Local.ymd(2022, 4, 4).and_hms(21, 20, 00),
        ));

        let s = r#"<TimeTable>
        <files id="0" uri="https://dev.micast.de/media.micast.de/spots/8487 - Big Cash Instore Spots DE 03-2019 Mix-03 - 01_09_15.wav"/>
        <spots file="0" start="2022-04-01T17:00:00" end="2022-06-01T23:59:00">
                <schedules start="07:00" end="22:00" weekdays="Mon" interval="2h"/>
                <schedules start="09:20" end="22:00" weekdays="Sun-Mon,Fri" interval="4h"/>
            </spots>
        </TimeTable>"#;

        let mut out = Scheduler::new();
        let _ = out.from_str(&s);
        let _ = out.process(Local.ymd(2022, 4, 4).and_hms(10, 30, 00));

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
        <files id="0" uri="https://dev.micast.de/media.micast.de/spots/8487 - Big Cash Instore Spots DE 03-2019 Mix-03 - 01_09_15.wav"/>
        <spots file="0" start="2022-04-01T17:00:00" end="2022-06-01T23:59:00">
                <schedules start="07:00" end="22:00" weekdays="Mon" interval="2h"/>
            </spots>
            <spots file="0" start="2022-04-01T17:00:00" end="2022-06-01T23:59:00">
                <schedules start="09:20" end="22:00" weekdays="Sun-Mon,Fri" interval="4h"/>
            </spots>
        </TimeTable>"#;

        let mut out = Scheduler::new();
        let _ = out.from_str(&s);
        let _ = out.process(Local.ymd(2022, 4, 4).and_hms(10, 30, 00));

        let sorted_output = out.sort();

        assert_eq!(sorted_output, vec!(
            ScheduledSpot { uri: "file:///test.mp3".to_string(), runs_at: Local.ymd(2022, 4, 4).and_hms(11, 00, 00) },
            ScheduledSpot { uri: "file:///test.mp3".to_string(), runs_at: Local.ymd(2022, 4, 4).and_hms(13, 00, 00) },
            ScheduledSpot { uri: "file:///ab_9_2.mp3".to_string(), runs_at: Local.ymd(2022, 4, 4).and_hms(13, 20, 00) },
            ScheduledSpot { uri: "file:///test.mp3".to_string(), runs_at: Local.ymd(2022, 4, 4).and_hms(15, 00, 00) },
            ScheduledSpot { uri: "file:///test.mp3".to_string(), runs_at: Local.ymd(2022, 4, 4).and_hms(17, 00, 00) },
            ScheduledSpot { uri: "file:///ab_9_2.mp3".to_string(), runs_at: Local.ymd(2022, 4, 4).and_hms(17, 20, 00) },
            ScheduledSpot { uri: "file:///test.mp3".to_string(), runs_at: Local.ymd(2022, 4, 4).and_hms(19, 00, 00) },
            ScheduledSpot { uri: "file:///test.mp3".to_string(), runs_at: Local.ymd(2022, 4, 4).and_hms(21, 00, 00) },
            ScheduledSpot { uri: "file:///ab_9_2.mp3".to_string(), runs_at: Local.ymd(2022, 4, 4).and_hms(21, 20, 00) },
            
        ));

    }
}