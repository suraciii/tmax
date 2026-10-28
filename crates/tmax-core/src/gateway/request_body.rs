//! Borrowed request overlays: retain opaque payloads and allocate only the final body.
use bytes::Bytes;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{MapAccess, SeqAccess, Visitor},
    ser::{SerializeMap, SerializeSeq},
};
use serde_json::value::RawValue;
use std::{borrow::Cow, collections::BTreeMap, fmt, io};

/// The caller validates the complete object and resolves effort conflicts first.
/// Compatibility normalization preserves the old DOM's last-wins object keys
/// and private number representation, including opaque provider extensions.
pub(super) fn rewrite(
    bytes: Bytes,
    model: &str,
    protocol: &str,
    effort: Option<&str>,
    normalize: bool,
) -> serde_json::Result<Bytes> {
    let object: RawObject<'_> = serde_json::from_slice(&bytes)?;
    let model_changed = !same_string(object.0.get("model").copied(), model)?;
    let chat_effort = effort.filter(|_| protocol == "chat-completions");
    let response_effort = effort.filter(|_| protocol == "responses");
    let chat_changed = match chat_effort {
        Some(effort) => !same_string(object.0.get("reasoning_effort").copied(), effort)?,
        None => false,
    };
    let (reasoning, response_changed) = match response_effort {
        Some(effort) => match object.0.get("reasoning") {
            Some(raw) if raw.get() == "null" => (RawObject::default(), true),
            Some(raw) if raw.get().starts_with('{') => {
                let reasoning: RawObject = serde_json::from_str(raw.get())?;
                let changed = !same_string(reasoning.0.get("effort").copied(), effort)?;
                (reasoning, changed)
            }
            // A nonobject value is preserved unchanged; no effort is injected.
            Some(_) => (RawObject::default(), false),
            None => (RawObject::default(), true),
        },
        None => (RawObject::default(), false),
    };
    if !model_changed && !chat_changed && !response_changed && !normalize {
        drop(reasoning);
        drop(object);
        return Ok(bytes);
    }
    let overlay = Overlay {
        object: &object,
        model: model_changed.then_some(model),
        chat_effort: chat_effort.filter(|_| chat_changed),
        reasoning: response_effort
            .filter(|_| response_changed)
            .map(|effort| Reasoning {
                object: &reasoning,
                effort,
                normalize,
            }),
        normalize,
    };
    // RawValue serialization writes an entire raw fragment as one write_all;
    // this sizing pass does not scan or copy unchanged text/base64 payloads.
    let mut size = OutputSize(0);
    serde_json::to_writer(&mut size, &overlay)?;
    let mut output = Vec::with_capacity(size.0);
    serde_json::to_writer(&mut output, &overlay)?;
    Ok(Bytes::from(output))
}

fn same_string(raw: Option<&RawValue>, expected: &str) -> serde_json::Result<bool> {
    let Some(raw) = raw.filter(|raw| raw.get().starts_with('"')) else {
        return Ok(false);
    };
    let value: JsonString<'_> = serde_json::from_str(raw.get())?;
    Ok(value.0 == expected)
}

/// Borrow unescaped keys/strings; allocate only when JSON escaping requires it.
struct JsonString<'a>(Cow<'a, str>);

impl<'de> Deserialize<'de> for JsonString<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StringVisitor;
        impl<'de> Visitor<'de> for StringVisitor {
            type Value = JsonString<'de>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON string")
            }

            fn visit_borrowed_str<E: serde::de::Error>(
                self,
                value: &'de str,
            ) -> Result<Self::Value, E> {
                Ok(JsonString(Cow::Borrowed(value)))
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(JsonString(Cow::Owned(value.to_owned())))
            }
        }
        deserializer.deserialize_str(StringVisitor)
    }
}

#[derive(Default)]
struct RawObject<'a>(BTreeMap<Cow<'a, str>, &'a RawValue>);

impl<'de> Deserialize<'de> for RawObject<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor;
        impl<'de> Visitor<'de> for ObjectVisitor {
            type Value = RawObject<'de>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut object = BTreeMap::new();
                while let Some(key) = map.next_key::<JsonString<'de>>()? {
                    object.insert(key.0, map.next_value::<&'de RawValue>()?);
                }
                Ok(RawObject(object))
            }
        }
        deserializer.deserialize_map(ObjectVisitor)
    }
}

struct Overlay<'a, 'body> {
    object: &'a RawObject<'body>,
    model: Option<&'a str>,
    chat_effort: Option<&'a str>,
    reasoning: Option<Reasoning<'a, 'body>>,
    normalize: bool,
}

impl Serialize for Overlay<'_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        for (key, raw) in &self.object.0 {
            match key.as_ref() {
                "model" if self.model.is_some() => continue,
                "reasoning_effort" if self.chat_effort.is_some() => continue,
                "reasoning" if self.reasoning.is_some() => continue,
                _ => {}
            }
            map.serialize_entry(key, &RawField(raw, self.normalize))?;
        }
        if let Some(model) = self.model {
            map.serialize_entry("model", model)?;
        }
        if let Some(effort) = self.chat_effort {
            map.serialize_entry("reasoning_effort", effort)?;
        }
        if let Some(reasoning) = &self.reasoning {
            map.serialize_entry("reasoning", reasoning)?;
        }
        map.end()
    }
}

struct Reasoning<'a, 'body> {
    object: &'a RawObject<'body>,
    effort: &'a str,
    normalize: bool,
}

impl Serialize for Reasoning<'_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        for (key, raw) in &self.object.0 {
            if key.as_ref() != "effort" {
                map.serialize_entry(key, &RawField(raw, self.normalize))?;
            }
        }
        map.serialize_entry("effort", self.effort)?;
        map.end()
    }
}

/// Only requests flagged by validation need recursive normalization. Scalars
/// remain raw, objects retain one borrowed value per decoded key, and arrays
/// stream one borrowed element at a time rather than allocating a second tree.
struct RawField<'a>(&'a RawValue, bool);

impl Serialize for RawField<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if !self.1 {
            return self.0.serialize(serializer);
        }
        match self.0.get().as_bytes()[0] {
            b'{' => {
                let object: RawObject<'_> =
                    serde_json::from_str(self.0.get()).map_err(serde::ser::Error::custom)?;
                // With arbitrary_precision, serde's DOM interprets this sole
                // private member as a number rather than an ordinary object.
                if object.0.len() == 1 && object.0.contains_key("$serde_json::private::Number") {
                    let number: serde_json::Number =
                        serde_json::from_str(self.0.get()).map_err(serde::ser::Error::custom)?;
                    return number.serialize(serializer);
                }
                let mut map = serializer.serialize_map(Some(object.0.len()))?;
                for (key, raw) in object.0 {
                    map.serialize_entry(&key, &RawField(raw, true))?;
                }
                map.end()
            }
            b'[' => {
                let mut deserializer = serde_json::Deserializer::from_str(self.0.get());
                deserializer
                    .deserialize_seq(ArraySerializer(serializer))
                    .map_err(serde::ser::Error::custom)?
            }
            _ => self.0.serialize(serializer),
        }
    }
}

struct ArraySerializer<S>(S);

impl<'de, S: Serializer> Visitor<'de> for ArraySerializer<S> {
    type Value = Result<S::Ok, S::Error>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON array")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut input: A) -> Result<Self::Value, A::Error> {
        let mut output = match self.0.serialize_seq(None) {
            Ok(output) => output,
            Err(error) => return Ok(Err(error)),
        };
        while let Some(raw) = input.next_element::<&'de RawValue>()? {
            if let Err(error) = output.serialize_element(&RawField(raw, true)) {
                return Ok(Err(error));
            }
        }
        Ok(output.end())
    }
}

struct OutputSize(usize);

impl io::Write for OutputSize {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self.0.checked_add(bytes.len()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "rewritten request is too large")
        })?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
