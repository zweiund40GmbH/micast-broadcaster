
use serde::{self, Deserialize, Serializer, Deserializer};
use chrono::Weekday;


pub fn serialize<S>(
    h_m: &Vec<Weekday>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{

    if h_m.is_empty() {
        return serializer.serialize_str("Mon-Sun")
    }

    let mut output_array: Vec<String> = Vec::new();
    let mut hm_array = h_m.clone();


    while hm_array.len() > 0 {
        let start_day = hm_array[0];
    
        let mut current_day = start_day;
        hm_array.retain(|&x| x != current_day);

        let mut next_day = current_day.succ();
        let mut inner_counter = 0;
        
        while hm_array.contains(&next_day) && next_day != current_day && inner_counter <= 7 {
            current_day = next_day;
            inner_counter += 1;
            hm_array.retain(|&x| x != current_day);
            next_day = current_day.succ();
        }

        if inner_counter > 0 {
            output_array.push(format!("{}-{}", start_day.to_string(), current_day.to_string()));
        } else {
            output_array.push(format!("{}", start_day.to_string()));
        }

    }

    serializer.serialize_str(&output_array.join(","))

}

pub fn deserialize<'de, D>(
    deserializer: D,
) -> Result<Vec<Weekday>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s == "" {
        return Ok(Vec::new())
    }

    let in_array: Vec<&str> = s.split(",").collect();

    let mut output_array: Vec<Weekday> = Vec::new();

    for day in in_array {
        if day.contains("-") {
            let from_to_day: Vec<&str> = day.split("-").collect();
            let mut start_day = from_to_day[0].parse::<Weekday>().unwrap();
            let end_day = from_to_day[1].parse::<Weekday>().unwrap();

            while start_day != end_day {
                output_array.push(start_day);
                start_day = start_day.succ();
            }
            
            output_array.push(end_day);
        } else {
            let parsed_day = day.parse::<Weekday>().unwrap(); 
            output_array.push(parsed_day);
        }
    }

    Ok(output_array)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use quick_xml::de::{from_str};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    pub struct TestStruct {
        #[serde(with = "super")]
        pub weekdays: Vec<Weekday>,
    }

    #[test]
    fn weekday_serialize() {
        let s = TestStruct {
            weekdays: vec![Weekday::Mon, Weekday::Tue, Weekday::Fri],
        };
        let out = quick_xml::se::to_string(&s).unwrap();
        assert_eq!(out, r#"<TestStruct weekdays="Mon-Tue,Fri"/>"#);
        
        let s = TestStruct {
            weekdays: vec![Weekday::Mon],
        };
        let out = quick_xml::se::to_string(&s).unwrap();
        assert_eq!(out, r#"<TestStruct weekdays="Mon"/>"#);

        let s = TestStruct {
            weekdays: vec![Weekday::Mon, Weekday::Wed, Weekday::Fri],
        };
        let out = quick_xml::se::to_string(&s).unwrap();
        assert_eq!(out, r#"<TestStruct weekdays="Mon,Wed,Fri"/>"#);

        let s = TestStruct {
            weekdays: vec![Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Fri],
        };
        let out = quick_xml::se::to_string(&s).unwrap();
        assert_eq!(out, r#"<TestStruct weekdays="Mon-Wed,Fri"/>"#);
    }

    #[test]
    fn weekday_deserialize() {
        let input_str = r#"<TestStruct weekdays="Mon-Fri"/>"#;
        let out: TestStruct = from_str(&input_str).unwrap();
        assert_eq!(out, TestStruct {
            weekdays: vec![Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri],
        });

        let input_str = r#"<TestStruct weekdays="Fri"/>"#;
        let out: TestStruct = from_str(&input_str).unwrap();
        assert_eq!(out, TestStruct {
            weekdays: vec![Weekday::Fri],
        });

        let input_str = r#"<TestStruct weekdays="Mon-Wed,Fri"/>"#;
        let out: TestStruct = from_str(&input_str).unwrap();
        assert_eq!(out, TestStruct {
            weekdays: vec![Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Fri],
        });

        let input_str = r#"<TestStruct weekdays="Mon,Wed,Fri"/>"#;
        let out: TestStruct = from_str(&input_str).unwrap();
        assert_eq!(out, TestStruct {
            weekdays: vec![Weekday::Mon, Weekday::Wed, Weekday::Fri],
        });
    }

    #[test]
    fn weekday_deserialize_special_case() {
        //Sun-Mon,Fri
        let input_str = r#"<TestStruct weekdays="Sun-Mon,Fri"/>"#;
        let out: TestStruct = from_str(&input_str).unwrap();
        assert_eq!(out, TestStruct {
            weekdays: vec![Weekday::Sun, Weekday::Mon, Weekday::Fri],
        });
    }

}
