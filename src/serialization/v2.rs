use caveat::{CaveatBuilder, CaveatType};
use error::MacaroonError;
use serialization::macaroon_builder::MacaroonBuilder;
use ByteString;
use Macaroon;

// Version 2 fields
const EOS: u8 = 0;
const LOCATION: u8 = 1;
const IDENTIFIER: u8 = 2;
const VID: u8 = 4;
const SIGNATURE: u8 = 6;

const VARINT_PACK_SIZE: usize = 128;

fn varint_size(size: usize) -> Vec<u8> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut my_size: usize = size;
    while my_size >= VARINT_PACK_SIZE {
        buffer.push(((my_size & (VARINT_PACK_SIZE - 1)) | VARINT_PACK_SIZE) as u8);
        my_size >>= 7;
    }
    buffer.push(my_size as u8);

    buffer
}

fn serialize_field(tag: u8, value: &[u8], buffer: &mut Vec<u8>) {
    buffer.push(tag);
    buffer.extend(varint_size(value.len()));
    buffer.extend(value);
}

pub fn serialize(macaroon: &Macaroon) -> Result<Vec<u8>, MacaroonError> {
    let mut buffer: Vec<u8> = Vec::new();
    buffer.push(2); // version
    if let Some(ref location) = macaroon.location() {
        serialize_field(LOCATION, &location.as_bytes().to_vec(), &mut buffer);
    };
    serialize_field(IDENTIFIER, &macaroon.identifier().0, &mut buffer);
    buffer.push(EOS);
    for caveat in macaroon.caveats() {
        match caveat.get_type() {
            CaveatType::FirstParty => {
                let first_party = caveat.as_first_party().unwrap();
                serialize_field(IDENTIFIER, &first_party.predicate().0, &mut buffer);
                buffer.push(EOS);
            }
            CaveatType::ThirdParty => {
                let third_party = caveat.as_third_party().unwrap();
                serialize_field(LOCATION, third_party.location().as_bytes(), &mut buffer);
                serialize_field(IDENTIFIER, &third_party.id().0, &mut buffer);
                serialize_field(VID, &third_party.verifier_id().0, &mut buffer);
                buffer.push(EOS);
            }
        }
    }
    buffer.push(EOS);
    serialize_field(SIGNATURE, macaroon.signature(), &mut buffer);
    Ok(buffer)
}

struct Deserializer<'r> {
    data: &'r [u8],
    index: usize,
}

impl<'r> Deserializer<'r> {
    pub fn new(data: &[u8]) -> Deserializer {
        Deserializer { data, index: 0 }
    }

    fn get_byte(&mut self) -> Result<u8, MacaroonError> {
        if self.index > self.data.len() - 1 {
            return Err(MacaroonError::DeserializationError(String::from(
                "Buffer overrun",
            )));
        }
        let byte = self.data[self.index];
        self.index += 1;
        Ok(byte)
    }

    pub fn get_tag(&mut self) -> Result<u8, MacaroonError> {
        self.get_byte()
    }

    pub fn get_eos(&mut self) -> Result<u8, MacaroonError> {
        let eos = self.get_byte()?;
        match eos {
            EOS => Ok(eos),
            _ => Err(MacaroonError::DeserializationError(String::from(
                "Expected EOS",
            ))),
        }
    }

    pub fn get_field(&mut self) -> Result<Vec<u8>, MacaroonError> {
        let size: usize = self.get_field_size()?;
        if size + self.index > self.data.len() {
            return Err(MacaroonError::DeserializationError(String::from(
                "Unexpected end of \
                 field",
            )));
        }

        let field: Vec<u8> = self.data[self.index..self.index + size].to_vec();
        self.index += size;
        Ok(field)
    }

    fn get_field_size(&mut self) -> Result<usize, MacaroonError> {
        let mut size: usize = 0;
        let mut shift: usize = 0;
        let mut byte: u8;
        while shift <= 63 {
            byte = self.get_byte()?;
            if byte & 128 != 0 {
                size |= ((byte & 127) << shift) as usize;
            } else {
                size |= (byte << shift) as usize;
                return Ok(size);
            }
            shift += 7;
        }
        Err(MacaroonError::DeserializationError(String::from(
            "Error in field size",
        )))
    }
}

pub fn deserialize(data: &[u8]) -> Result<Macaroon, MacaroonError> {
    let mut builder: MacaroonBuilder = MacaroonBuilder::new();
    let mut deserializer: Deserializer = Deserializer::new(data);
    if deserializer.get_byte()? != 2 {
        return Err(MacaroonError::DeserializationError(String::from(
            "Wrong version number",
        )));
    }
    let mut tag: u8 = deserializer.get_tag()?;
    match tag {
        LOCATION => builder.set_location(&String::from_utf8(deserializer.get_field()?)?),
        IDENTIFIER => builder.set_identifier(ByteString(deserializer.get_field()?)),
        _ => {
            return Err(MacaroonError::DeserializationError(String::from(
                "Identifier not found",
            )))
        }
    }
    if builder.has_location() {
        tag = deserializer.get_tag()?;
        match tag {
            IDENTIFIER => {
                builder.set_identifier(ByteString(deserializer.get_field()?));
            }
            _ => {
                return Err(MacaroonError::DeserializationError(String::from(
                    "Identifier not \
                     found",
                )))
            }
        }
    }
    deserializer.get_eos()?;
    tag = deserializer.get_tag()?;
    while tag != EOS {
        let mut caveat_builder: CaveatBuilder = CaveatBuilder::new();
        match tag {
            LOCATION => {
                let field: Vec<u8> = deserializer.get_field()?;
                caveat_builder.add_location(String::from_utf8(field)?);
            }
            IDENTIFIER => caveat_builder.add_id(ByteString(deserializer.get_field()?)),
            _ => {
                return Err(MacaroonError::DeserializationError(String::from(
                    "Caveat identifier \
                     not found",
                )))
            }
        }
        if caveat_builder.has_location() {
            tag = deserializer.get_tag()?;
            match tag {
                IDENTIFIER => {
                    let field: Vec<u8> = deserializer.get_field()?;
                    caveat_builder.add_id(ByteString(field));
                }
                _ => {
                    return Err(MacaroonError::DeserializationError(String::from(
                        "Caveat identifier \
                         not found",
                    )))
                }
            }
        }
        tag = deserializer.get_tag()?;
        match tag {
            VID => {
                let field: Vec<u8> = deserializer.get_field()?;
                caveat_builder.add_verifier_id(ByteString(field));
                builder.add_caveat(caveat_builder.build()?);
                deserializer.get_eos()?;
                tag = deserializer.get_tag()?;
            }
            EOS => {
                builder.add_caveat(caveat_builder.build()?);
                tag = deserializer.get_tag()?;
            }
            _ => {
                return Err(MacaroonError::DeserializationError(String::from(
                    "Unexpected caveat \
                     tag found",
                )))
            }
        }
    }
    tag = deserializer.get_tag()?;
    if tag == SIGNATURE {
        let sig: Vec<u8> = deserializer.get_field()?;
        if sig.len() != 32 {
            return Err(MacaroonError::DeserializationError(String::from(
                "Bad signature length",
            )));
        }
        builder.set_signature(&sig);
    } else {
        return Err(MacaroonError::DeserializationError(String::from(
            "Unexpected tag found",
        )));
    }
    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use caveat;
    use serialization::macaroon_builder::MacaroonBuilder;
    use ByteString;
    use Macaroon;

    #[test]
    fn test_deserialize() {
        const SERIALIZED: &str = "AgETaHR0cDovL2V4YW1wbGUub3JnLwIFa2V5aWQAAhRhY2NvdW50ID0gMzczNTkyODU1OQACDHVzZXIgPSBhbGljZQAABiBL6WfNHqDGsmuvakqU7psFsViG2guoXoxCqTyNDhJe_A==";
        const SIGNATURE: [u8; 32] = [
            75, 233, 103, 205, 30, 160, 198, 178, 107, 175, 106, 74, 148, 238, 155, 5, 177, 88,
            134, 218, 11, 168, 94, 140, 66, 169, 60, 141, 14, 18, 94, 252,
        ];
        let serialized: Vec<u8> = base64::decode_config(SERIALIZED, base64::URL_SAFE).unwrap();
        let macaroon = super::deserialize(&serialized).unwrap();
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
    fn test_serialize() {
        const SERIALIZED: &str = "AgETaHR0cDovL2V4YW1wbGUub3JnLwIFa2V5aWQAAhRhY2NvdW50ID0gMzczNTkyODU1OQACDHVzZXIgPSBhbGljZQAABiBL6WfNHqDGsmuvakqU7psFsViG2guoXoxCqTyNDhJe_A==";
        const SIGNATURE: [u8; 32] = [
            75, 233, 103, 205, 30, 160, 198, 178, 107, 175, 106, 74, 148, 238, 155, 5, 177, 88,
            134, 218, 11, 168, 94, 140, 66, 169, 60, 141, 14, 18, 94, 252,
        ];
        let mut builder = MacaroonBuilder::new();
        builder.add_caveat(Box::new(caveat::new_first_party(
            "account = 3735928559".into(),
        )));
        builder.add_caveat(Box::new(caveat::new_first_party("user = alice".into())));
        builder.set_location("http://example.org/");
        builder.set_identifier("keyid".into());
        builder.set_signature(&SIGNATURE);
        let serialized = super::serialize(&builder.build().unwrap()).unwrap();
        assert_eq!(
            base64::decode_config(SERIALIZED, base64::URL_SAFE).unwrap(),
            serialized
        );
    }

    #[test]
    fn test_serialize_deserialize() {
        let mut macaroon = Macaroon::create("http://example.org/", b"key", "keyid".into()).unwrap();
        macaroon.add_first_party_caveat("account = 3735928559".into());
        macaroon.add_first_party_caveat("user = alice".into());
        macaroon.add_third_party_caveat("https://auth.mybank.com", b"caveat key", "caveat".into());
        let serialized = super::serialize(&macaroon).unwrap();
        macaroon = super::deserialize(&serialized).unwrap();
        assert_eq!("http://example.org/", &macaroon.location().unwrap());
        assert_eq!(ByteString::from("keyid"), macaroon.identifier());
        assert_eq!(3, macaroon.caveats().len());
        assert_eq!(
            ByteString::from("account = 3735928559"),
            macaroon.caveats()[0].as_first_party().unwrap().predicate()
        );
        assert_eq!(
            ByteString::from("user = alice"),
            macaroon.caveats()[1].as_first_party().unwrap().predicate()
        );
        assert_eq!(
            ByteString::from("caveat"),
            macaroon.caveats()[2].as_third_party().unwrap().id()
        );
        assert_eq!(
            "https://auth.mybank.com",
            macaroon.caveats()[2].as_third_party().unwrap().location()
        );
    }
}
