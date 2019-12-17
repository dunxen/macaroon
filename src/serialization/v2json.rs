use caveat::{CaveatBuilder, CaveatType};
use error::MacaroonError;
use serde::{Deserialize, Serialize};
use serde_json;
use serialization::macaroon_builder::MacaroonBuilder;
use std::str;
use ByteString;
use Macaroon;

#[derive(Debug, Default, Deserialize, Serialize)]
struct Caveat {
    i: Option<String>,
    i64: Option<ByteString>,
    l: Option<String>,
    l64: Option<String>,
    v: Option<String>,
    v64: Option<ByteString>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct Serialization {
    v: u8,
    i: Option<String>,
    i64: Option<ByteString>,
    l: Option<String>,
    l64: Option<String>,
    c: Vec<Caveat>,
    s: Option<Vec<u8>>,
    s64: Option<String>,
}

impl Serialization {
    fn from_macaroon(macaroon: Macaroon) -> Result<Serialization, MacaroonError> {
        let mut serialized: Serialization = Serialization {
            v: 2,
            i: None,
            i64: Some(macaroon.identifier().clone()),
            l: macaroon.location().clone(),
            l64: None,
            c: Vec::new(),
            s: None,
            s64: Some(base64::encode_config(
                macaroon.signature(),
                base64::URL_SAFE,
            )),
        };
        for caveat in macaroon.caveats() {
            match caveat.get_type() {
                CaveatType::FirstParty => {
                    let first_party = caveat.as_first_party().unwrap();
                    let serialized_caveat: Caveat = Caveat {
                        i: None,
                        i64: Some(first_party.predicate()),
                        l: None,
                        l64: None,
                        v: None,
                        v64: None,
                    };
                    serialized.c.push(serialized_caveat);
                }
                CaveatType::ThirdParty => {
                    let third_party = caveat.as_third_party().unwrap();
                    let serialized_caveat: Caveat = Caveat {
                        i: None,
                        i64: Some(third_party.id()),
                        l: Some(third_party.location()),
                        l64: None,
                        v: None,
                        v64: Some(third_party.verifier_id()),
                    };
                    serialized.c.push(serialized_caveat);
                }
            }
        }

        Ok(serialized)
    }
}

impl Macaroon {
    fn from_json(ser: Serialization) -> Result<Macaroon, MacaroonError> {
        if ser.i.is_some() && ser.i64.is_some() {
            return Err(MacaroonError::DeserializationError(String::from(
                "Found i and i64 fields",
            )));
        }
        if ser.l.is_some() && ser.l64.is_some() {
            return Err(MacaroonError::DeserializationError(String::from(
                "Found l and l64 fields",
            )));
        }
        if ser.s.is_some() && ser.s64.is_some() {
            return Err(MacaroonError::DeserializationError(String::from(
                "Found s and s64 fields",
            )));
        }

        let mut builder: MacaroonBuilder = MacaroonBuilder::new();
        builder.set_identifier(match ser.i {
            Some(id) => id.into(),
            None => match ser.i64 {
                Some(id) => id,
                None => {
                    return Err(MacaroonError::DeserializationError(String::from(
                        "No identifier \
                         found",
                    )))
                }
            },
        });

        match ser.l {
            Some(loc) => builder.set_location(&loc),
            None => {
                if let Some(loc) = ser.l64 {
                    builder.set_location(&String::from_utf8(base64::decode_config(
                        &loc,
                        base64::URL_SAFE,
                    )?)?)
                }
            }
        };

        builder.set_signature(&match ser.s {
            Some(sig) => sig,
            None => match ser.s64 {
                Some(sig) => base64::decode_config(&sig, base64::URL_SAFE)?,
                None => {
                    return Err(MacaroonError::DeserializationError(String::from(
                        "No signature \
                         found",
                    )))
                }
            },
        });

        let mut caveat_builder: CaveatBuilder = CaveatBuilder::new();
        for c in ser.c {
            caveat_builder.add_id(match c.i {
                Some(id) => id.into(),
                None => match c.i64 {
                    Some(id64) => id64,
                    None => {
                        return Err(MacaroonError::DeserializationError(String::from(
                            "No caveat \
                             ID found",
                        )))
                    }
                },
            });
            match c.l {
                Some(loc) => caveat_builder.add_location(loc),
                None => {
                    if let Some(loc64) = c.l64 {
                        caveat_builder.add_location(String::from_utf8(base64::decode_config(
                            &loc64,
                            base64::URL_SAFE,
                        )?)?)
                    }
                }
            };
            match c.v {
                Some(vid) => caveat_builder.add_verifier_id(vid.into()),
                None => {
                    if let Some(vid64) = c.v64 {
                        caveat_builder.add_verifier_id(vid64)
                    }
                }
            };
            builder.add_caveat(caveat_builder.build()?);
            caveat_builder = CaveatBuilder::new();
        }

        Ok(builder.build()?)
    }
}

pub fn serialize(macaroon: &Macaroon) -> Result<Vec<u8>, MacaroonError> {
    let serialized: String =
        serde_json::to_string(&Serialization::from_macaroon(macaroon.clone())?)?;
    Ok(serialized.into_bytes())
}

pub fn deserialize(data: &[u8]) -> Result<Macaroon, MacaroonError> {
    let v2j: Serialization = serde_json::from_slice(data)?;
    Macaroon::from_json(v2j)
}

#[cfg(test)]
mod tests {
    use super::super::Format;
    use ByteString;
    use Macaroon;

    const SERIALIZED_JSON: &str = "{\"v\":2,\"l\":\"http://example.org/\",\"i\":\"keyid\",\
                                   \"c\":[{\"i\":\"account = 3735928559\"},{\"i\":\"user = \
                                   alice\"}],\"s64\":\
                                   \"S-lnzR6gxrJrr2pKlO6bBbFYhtoLqF6MQqk8jQ4SXvw\"}";
    const SIGNATURE: [u8; 32] = [
        75, 233, 103, 205, 30, 160, 198, 178, 107, 175, 106, 74, 148, 238, 155, 5, 177, 88, 134,
        218, 11, 168, 94, 140, 66, 169, 60, 141, 14, 18, 94, 252,
    ];

    #[test]
    fn test_deserialize() {
        let serialized_json: Vec<u8> = SERIALIZED_JSON.as_bytes().to_vec();
        let macaroon = super::deserialize(&serialized_json).unwrap();
        assert_eq!("http://example.org/", &macaroon.location().unwrap());
        assert_eq!(ByteString::from("keyid"), macaroon.identifier());
        assert_eq!(2, macaroon.caveats().len());
        assert_eq!(
            ByteString::from("account = 3735928559"),
            macaroon.caveats()[0].as_first_party().unwrap().predicate()
        );
        assert_eq!(
            ByteString::from("user = alice"),
            macaroon.caveats()[1].as_first_party().unwrap().predicate()
        );
        assert_eq!(SIGNATURE.to_vec(), macaroon.signature());
    }

    #[test]
    fn test_serialize_deserialize() {
        let mut macaroon =
            Macaroon::create("http://example.org/", &SIGNATURE, "keyid".into()).unwrap();
        macaroon.add_first_party_caveat("user = alice".into());
        macaroon.add_third_party_caveat("https://auth.mybank.com/", b"my key", "keyid".into());
        let serialized = macaroon.serialize(Format::V2JSON).unwrap();
        let other = Macaroon::deserialize(&serialized).unwrap();
        assert_eq!(macaroon, other);
    }
}
