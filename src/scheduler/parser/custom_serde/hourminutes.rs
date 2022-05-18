use serde::{self, Deserialize, Serializer, Deserializer};

pub fn serialize<S>(
    h_m: &Option<(u16,u16)>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if let Some(hm) = h_m {

        if hm.0 >= 24 {
            return Err(serde::ser::Error::custom(format!("Hour is invalid (23 is max): {}", hm.0)));
        }

        if hm.1 >= 60 {
            return Err(serde::ser::Error::custom(format!("Minute is invalid (59 is max): {}", hm.1)));
        }

        let s = format!("{:0>2}:{:0>2}", hm.0, hm.1);
        return serializer.serialize_str(&s)
    }

    serializer.serialize_str("")
    
}

// The signature of a deserialize_with function must follow the pattern:
//
//    fn deserialize<'de, D>(D) -> Result<T, D::Error>
//    where
//        D: Deserializer<'de>
//
// although it may also be generic over the output types T.
pub fn deserialize<'de, D>(
    deserializer: D,
) -> Result<Option<(u16,u16)>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;

    if s == "" {
        return Ok(None)
    }

    let res: Vec<&str> = s.split(":").collect();
    if res.len() != 2 {
        //bail!("value is to long");
        return Err(serde::de::Error::custom(format!("invalid HH::mm format for: {}", s)));
    }

    if res[0].is_empty() {
        return Err(serde::de::Error::custom(format!("Hour is empty: {}", s)));
    }

    if res[1].is_empty() {
        return Err(serde::de::Error::custom(format!("Minute is empty: {}", s)));
    }


    let h = res[0].parse::<u16>();
    let m = res[1].parse::<u16>();




    if let Err(e) = &h {
        return Err(serde::de::Error::custom(format!("Hour could not converted: {}", e)));
    }

    if let Err(e) = &m {
        //bail!("invalid minute: {}", e);
        return Err(serde::de::Error::custom(format!("Minute could not converted: {}", e)));
    }

    let h = h.unwrap();
    if h >= 24 {
        return Err(serde::de::Error::custom(format!("Hour is invalid (23 is max): {}", h)));
    }

    let m = m.unwrap();
    if m >= 60 {
        return Err(serde::de::Error::custom(format!("Minute is invalid (59 is max): {}", m)));
    }

    Ok(Some((h,m)))
}



#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use quick_xml::de::{from_str};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    pub struct TestStruct {
        #[serde(with = "super")]
        pub start: Option<(u16,u16)>,
    }

    #[test]
    fn hourminutes_serialize() {
        let s = TestStruct {
            start: Some((7,33).into()),
        };
        let out = quick_xml::se::to_string(&s).unwrap();
        assert_eq!(out, r#"<TestStruct start="07:33"/>"#);
        
        let s = TestStruct {
            start: Some((10,20).into()),
        };
        let out = quick_xml::se::to_string(&s).unwrap();
        assert_eq!(out, r#"<TestStruct start="10:20"/>"#);

        let s = TestStruct {
            start: Some((1,1).into()),
        };
        let out = quick_xml::se::to_string(&s).unwrap();
        assert_eq!(out, r#"<TestStruct start="01:01"/>"#);
    }

    #[test]
    fn hourminutes_deserialize() {
        
        let input_str = r#"<TestStruct start="10:20"/>"#;
        let out: TestStruct = from_str(&input_str).unwrap();
        assert_eq!(out, TestStruct {
            start: Some((10,20).into()),
        });

        let input_str = r#"<TestStruct start="23:59"/>"#;
        let out: TestStruct = from_str(&input_str).unwrap();
        assert_eq!(out, TestStruct {
            start: Some((23,59).into()),
        });

    }

    #[test]
    fn hourminutes_deserialize_special_case() {
        
        let input_str = r#"<TestStruct start="09:20"/>"#;
        let out: TestStruct = from_str(&input_str).unwrap();
        assert_eq!(out, TestStruct {
            start: Some((09,20).into()),
        });


    }

    #[test]
    fn hourminutes_deserialize_error_handling() {
        
        let input_str = r#"<TestStruct start=":20"/>"#;
        let out = from_str::<TestStruct>(&input_str);

        assert!(out.is_err());

        let input_str = r#"<TestStruct start=""/>"#;
        let out = from_str::<TestStruct>(&input_str);
        assert_eq!(out.unwrap(), TestStruct {
            start: None,
        });

        let input_str = r#"<TestStruct start="2:"/>"#;
        let out = from_str::<TestStruct>(&input_str);
        assert!(out.is_err());

        let input_str = r#"<TestStruct start="24:20"/>"#;
        let out = from_str::<TestStruct>(&input_str);
        assert!(out.is_err());

        let input_str = r#"<TestStruct start="23:60"/>"#;
        let out = from_str::<TestStruct>(&input_str);
        assert!(out.is_err());

    }

    #[test]
    fn hourminutes_serialize_error_handling() {
        let s = TestStruct {
            start: Some((0,70).into()),
        };
        let out = quick_xml::se::to_string(&s);
        assert!(out.is_err());
        
        let s = TestStruct {
            start: Some((24,20).into()),
        };
        let out = quick_xml::se::to_string(&s);
        assert!(out.is_err());
    }

}
