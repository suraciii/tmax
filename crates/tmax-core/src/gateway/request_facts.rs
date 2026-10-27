//! Strict wire validation and the finite facts used by native request policy.
use serde::Deserialize;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt,
};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RequestFacts {
    pub model: Option<String>,
    pub normalization_required: bool,
    pub(crate) tool_use: bool,
    pub(crate) thinking_enabled: bool,
    pub(crate) thinking_disabled: bool,
    pub(crate) temperature: bool,
    pub(crate) output_budgets: [Option<String>; 3],
    pub(crate) structured: bool,
    pub(crate) input_modalities: BTreeSet<&'static str>,
    pub(crate) output_modalities: BTreeSet<String>,
    pub(crate) reasoning_effort: Effort,
    pub(crate) nested_effort: Effort,
    pub(crate) reasoning_invalid: bool,
    pub(crate) remote_state: bool,
    pub(crate) unbound_resources: bool,
    pub(crate) background: bool,
    pub(crate) deepseek_state: bool,
    messages: Items,
    input: Items,
    store_false: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) enum Effort {
    #[default]
    Missing,
    Null,
    Invalid,
    String(String),
}
impl Effort {
    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }
    pub(crate) fn present(&self) -> bool {
        !matches!(self, Self::Missing)
    }
}

impl RequestFacts {
    pub fn parse(bytes: &[u8]) -> serde_json::Result<Self> {
        let mut deserializer = serde_json::Deserializer::from_slice(bytes);
        let mut duplicate_keys = false;
        let value = Seed {
            context: Context::Root,
            duplicates: &mut duplicate_keys,
        }
        .deserialize(&mut deserializer)?;
        deserializer.end()?;
        Self::finish(value, duplicate_keys)
    }

    /// Generated requests and tests use the same extractor without serializing a DOM.
    pub fn from_value(value: serde_json::Value) -> serde_json::Result<Self> {
        let mut duplicate_keys = false;
        let value = Seed {
            context: Context::Root,
            duplicates: &mut duplicate_keys,
        }
        .deserialize(value)?;
        Self::finish(value, duplicate_keys)
    }

    fn finish(mut root: Node, duplicate_keys: bool) -> serde_json::Result<Self> {
        if root.kind != Kind::Object {
            return Err(<serde_json::Error as de::Error>::custom(
                "request must be a JSON object",
            ));
        }
        let messages = root.take("messages").items();
        let input = root.take("input").items();
        let mut reasoning = root.take("reasoning");
        let thinking = root.take("thinking");
        let response_format = root.take("response_format").take("type");
        let text_format = root.take("text").take("format").take("type");
        let format = if response_format.kind != Kind::Missing {
            response_format
        } else {
            text_format
        };
        let top_effort = root.take("reasoning_effort").effort();
        let nested_effort = reasoning.take("effort").effort();
        let tools = root.take("tools").items();
        let functions = root.take("functions").items();
        let mut modalities = messages.modalities.clone();
        modalities.extend(input.modalities.iter().copied());
        if input.string {
            modalities.insert("text");
        }
        let previous = root.take("previous_response_id");
        let conversation = root.take("conversation");
        let background = root.take("background");
        let store = root.take("store");
        let store_false = store.kind == Kind::False;
        Ok(Self {
            model: root.take("model").text,
            normalization_required: duplicate_keys,
            tool_use: tools.nonempty
                || functions.nonempty
                || messages.tools
                || input.function_calls,
            thinking_enabled: matches!(thinking.field("type").atom, Atom::Enabled | Atom::Adaptive),
            thinking_disabled: thinking.field("type").atom == Atom::Disabled,
            temperature: root.take("temperature").nonnull(),
            output_budgets: [
                root.take("max_tokens").number,
                root.take("max_output_tokens").number,
                root.take("max_completion_tokens").number,
            ],
            structured: format.atom == Atom::JsonSchema,
            input_modalities: modalities,
            output_modalities: root.take("modalities").strings,
            reasoning_effort: top_effort,
            nested_effort,
            reasoning_invalid: !matches!(reasoning.kind, Kind::Missing | Kind::Null | Kind::Object),
            remote_state: previous.nonnull()
                || conversation.nonnull()
                || background.kind == Kind::True
                || input.references,
            unbound_resources: root.take("prompt").take("id").nonnull()
                || tools.resources
                || messages.resources
                || input.resources,
            background: background.kind == Kind::True,
            deepseek_state: [previous, conversation, store, background]
                .iter()
                .any(|v| v.nonnull() && v.kind != Kind::False),
            messages,
            input,
            store_false,
        })
    }

    pub(crate) fn effort(&self, protocol: &str) -> &Effort {
        match protocol {
            "chat-completions" => &self.reasoning_effort,
            "responses" => &self.nested_effort,
            _ => &Effort::Missing,
        }
    }
    pub(crate) fn independent(&self, protocol: &str) -> bool {
        !self.remote_state
            && match protocol {
                "chat-completions" | "messages" => self.messages.nonempty,
                "responses" => self.store_false && (self.input.array || self.input.string),
                _ => false,
            }
    }
    pub(crate) fn new_session(&self, protocol: &str) -> bool {
        let items = match protocol {
            "chat-completions" | "messages" => &self.messages,
            "responses" => &self.input,
            _ => return false,
        };
        items.string || (items.array && items.valid_roles && items.users == 1)
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Items {
    array: bool,
    string: bool,
    nonempty: bool,
    valid_roles: bool,
    users: u8,
    tools: bool,
    function_calls: bool,
    references: bool,
    resources: bool,
    modalities: BTreeSet<&'static str>,
}
impl Items {
    fn include(&mut self, item: Node, context: Context) {
        self.nonempty = true;
        if context == Context::Tools {
            self.resources |= item.field("type").atom == Atom::FileSearch
                && item.field("vector_store_ids").array.nonempty;
            return;
        }
        if context == Context::Discard {
            return;
        }
        if context == Context::Content {
            self.resources |= item.unbound();
            if let Some(modality) = item.modality() {
                self.modalities.insert(modality);
            }
            self.tools |= matches!(item.field("type").atom, Atom::ToolUse | Atom::ToolResult);
            return;
        }
        match item.field("role").atom {
            Atom::System | Atom::Developer => {}
            Atom::User => self.users = self.users.saturating_add(1).min(2),
            _ => self.valid_roles = false,
        }
        self.tools |=
            item.field("role").atom == Atom::Tool || item.field("tool_calls").array.nonempty;
        self.function_calls |= matches!(
            item.field("type").atom,
            Atom::FunctionCall | Atom::FunctionOutput
        );
        self.references |= item.field("type").atom == Atom::ItemReference;
        self.resources |= item.unbound();
        if let Some(modality) = item.modality() {
            self.modalities.insert(modality);
        }
        let content = item.field("content");
        if content.kind == Kind::String {
            self.modalities.insert("text");
        }
        self.modalities
            .extend(content.array.modalities.iter().copied());
        self.resources |= content.array.resources;
        self.tools |= content.array.tools;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Kind {
    #[default]
    Missing,
    Null,
    False,
    True,
    String,
    Number,
    Object,
    Array,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Atom {
    #[default]
    Other,
    System,
    Developer,
    User,
    Tool,
    FunctionCall,
    FunctionOutput,
    ItemReference,
    ToolUse,
    ToolResult,
    Text,
    Image,
    Audio,
    Document,
    InputFile,
    File,
    Video,
    Pdf,
    FileSearch,
    Enabled,
    Adaptive,
    Disabled,
    JsonSchema,
}
impl Atom {
    fn parse(value: &str) -> Self {
        match value {
            "system" => Self::System,
            "developer" => Self::Developer,
            "user" => Self::User,
            "tool" => Self::Tool,
            "function_call" => Self::FunctionCall,
            "function_call_output" => Self::FunctionOutput,
            "item_reference" => Self::ItemReference,
            "tool_use" => Self::ToolUse,
            "tool_result" => Self::ToolResult,
            "text" | "input_text" => Self::Text,
            "image" | "image_url" | "input_image" => Self::Image,
            "audio" | "input_audio" => Self::Audio,
            "document" => Self::Document,
            "input_file" => Self::InputFile,
            "file" => Self::File,
            "video" | "input_video" => Self::Video,
            "application/pdf" => Self::Pdf,
            "file_search" => Self::FileSearch,
            "enabled" => Self::Enabled,
            "adaptive" => Self::Adaptive,
            "disabled" => Self::Disabled,
            "json_schema" => Self::JsonSchema,
            _ => Self::Other,
        }
    }
}
#[derive(Debug, Default)]
struct Node {
    kind: Kind,
    atom: Atom,
    text: Option<String>,
    number: Option<String>,
    fields: BTreeMap<&'static str, Node>,
    array: Items,
    strings: BTreeSet<String>,
}
static ABSENT: Node = Node {
    kind: Kind::Missing,
    atom: Atom::Other,
    text: None,
    number: None,
    fields: BTreeMap::new(),
    array: Items {
        array: false,
        string: false,
        nonempty: false,
        valid_roles: false,
        users: 0,
        tools: false,
        function_calls: false,
        references: false,
        resources: false,
        modalities: BTreeSet::new(),
    },
    strings: BTreeSet::new(),
};
impl Node {
    fn field(&self, key: &str) -> &Node {
        self.fields.get(key).unwrap_or(&ABSENT)
    }
    fn take(&mut self, key: &str) -> Node {
        self.fields.remove(key).unwrap_or_default()
    }
    fn nonnull(&self) -> bool {
        !matches!(self.kind, Kind::Missing | Kind::Null)
    }
    fn items(self) -> Items {
        Items {
            string: self.kind == Kind::String,
            ..self.array
        }
    }
    fn effort(self) -> Effort {
        match self.kind {
            Kind::Missing => Effort::Missing,
            Kind::Null => Effort::Null,
            Kind::String => Effort::String(self.text.unwrap_or_default()),
            _ => Effort::Invalid,
        }
    }
    fn modality(&self) -> Option<&'static str> {
        match self.field("type").atom {
            Atom::Text => Some("text"),
            Atom::Image => Some("image"),
            Atom::Audio => Some("audio"),
            Atom::Video => Some("video"),
            Atom::Document | Atom::InputFile
                if self.field("source").field("media_type").atom == Atom::Pdf =>
            {
                Some("pdf")
            }
            _ => None,
        }
    }
    fn unbound(&self) -> bool {
        matches!(
            self.field("type").atom,
            Atom::InputFile | Atom::File | Atom::Document
        ) && (self.field("file_id").nonnull()
            || self.field("file").field("file_id").nonnull()
            || self.field("source").field("file_id").nonnull())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Context {
    Root,
    Items,
    Item,
    Content,
    Tools,
    Tool,
    Source,
    File,
    Reasoning,
    Thinking,
    Format,
    Text,
    Prompt,
    Modalities,
    Retain,
    Number,
    Scalar,
    Discard,
}
impl Context {
    fn field(self, key: &str) -> Option<(&'static str, Self)> {
        use Context::*;
        Some(match (self, key) {
            (Root, "model") => ("model", Retain),
            (Root, "reasoning_effort") => ("reasoning_effort", Retain),
            (Root, "messages") => ("messages", Items),
            (Root, "input") => ("input", Items),
            (Root, "tools") => ("tools", Tools),
            (Root, "functions") => ("functions", Discard),
            (Root, "reasoning") => ("reasoning", Reasoning),
            (Root, "thinking") => ("thinking", Thinking),
            (Root, "response_format") => ("response_format", Format),
            (Root, "text") => ("text", Text),
            (Root, "prompt") => ("prompt", Prompt),
            (Root, "modalities") => ("modalities", Modalities),
            (Root, "max_tokens") => ("max_tokens", Number),
            (Root, "max_output_tokens") => ("max_output_tokens", Number),
            (Root, "max_completion_tokens") => ("max_completion_tokens", Number),
            (Root, "temperature") => ("temperature", Scalar),
            (Root, "previous_response_id") => ("previous_response_id", Scalar),
            (Root, "conversation") => ("conversation", Scalar),
            (Root, "background") => ("background", Scalar),
            (Root, "store") => ("store", Scalar),
            (Item, "role") => ("role", Scalar),
            (Item | Tool | Thinking | Format, "type") => ("type", Scalar),
            (Item, "content") => ("content", Content),
            (Item, "tool_calls") => ("tool_calls", Discard),
            (Item, "file_id") => ("file_id", Scalar),
            (Item, "file") => ("file", File),
            (Item, "source") => ("source", Source),
            (File | Source, "file_id") => ("file_id", Scalar),
            (Source, "media_type") => ("media_type", Scalar),
            (Tool, "vector_store_ids") => ("vector_store_ids", Discard),
            (Reasoning, "effort") => ("effort", Retain),
            (Text, "format") => ("format", Format),
            (Prompt, "id") => ("id", Scalar),
            _ => return None,
        })
    }
    fn element(self) -> Self {
        match self {
            Self::Items | Self::Content => Self::Item,
            Self::Tools => Self::Tool,
            Self::Modalities => Self::Retain,
            _ => Self::Discard,
        }
    }
}
struct Seed<'a> {
    context: Context,
    duplicates: &'a mut bool,
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Node;
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Node, D::Error> {
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Node;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Node, E> {
        Ok(Node {
            kind: Kind::Null,
            ..Node::default()
        })
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Node, E> {
        Ok(Node {
            kind: if value { Kind::True } else { Kind::False },
            ..Node::default()
        })
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Node, E> {
        let number = if self.context == Context::Number {
            value.to_string()
        } else {
            String::new()
        };
        self.numeric(number)
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Node, E> {
        let number = if self.context == Context::Number {
            value.to_string()
        } else {
            String::new()
        };
        self.numeric(number)
    }
    fn visit_i128<E: de::Error>(self, value: i128) -> Result<Node, E> {
        *self.duplicates = true;
        let number = if self.context == Context::Number {
            value.to_string()
        } else {
            String::new()
        };
        self.numeric(number)
    }
    fn visit_u128<E: de::Error>(self, value: u128) -> Result<Node, E> {
        *self.duplicates = true;
        let number = if self.context == Context::Number {
            value.to_string()
        } else {
            String::new()
        };
        self.numeric(number)
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Node, E> {
        let number = if self.context == Context::Number {
            value.to_string()
        } else {
            String::new()
        };
        self.numeric(number)
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Node, E> {
        Ok(Node {
            kind: Kind::String,
            atom: Atom::parse(value),
            text: (self.context == Context::Retain).then(|| value.to_owned()),
            ..Node::default()
        })
    }
    fn visit_string<E: de::Error>(self, value: String) -> Result<Node, E> {
        let atom = Atom::parse(&value);
        Ok(Node {
            kind: Kind::String,
            atom,
            text: (self.context == Context::Retain).then_some(value),
            ..Node::default()
        })
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Node, A::Error> {
        let mut node = Node {
            kind: Kind::Array,
            array: Items {
                array: true,
                valid_roles: true,
                ..Items::default()
            },
            ..Node::default()
        };
        while let Some(item) = access.next_element_seed(Seed {
            context: self.context.element(),
            duplicates: self.duplicates,
        })? {
            if self.context == Context::Modalities {
                node.array.nonempty = true;
                if let Some(value) = item.text {
                    node.strings.insert(value);
                }
            } else {
                node.array.include(item, self.context);
            }
        }
        Ok(node)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Node, A::Error> {
        let mut node = Node {
            kind: Kind::Object,
            ..Node::default()
        };
        let mut keys = HashSet::new();
        while let Some(Key(key)) = access.next_key::<Key<'de>>()? {
            // serde_json's arbitrary-precision numbers arrive through this private map.
            if keys.is_empty() && key == "$serde_json::private::Number" {
                let number: String = access.next_value()?;
                number
                    .parse::<serde_json::Number>()
                    .map_err(de::Error::custom)?;
                *self.duplicates = true;
                if access.next_key::<Key<'de>>()?.is_some() {
                    return Err(de::Error::custom("unexpected number map field"));
                }
                return self.numeric(number);
            }
            let field = self.context.field(&key);
            if !keys.insert(key) {
                *self.duplicates = true;
            }
            let value = access.next_value_seed(Seed {
                context: field.map_or(Context::Discard, |(_, context)| context),
                duplicates: self.duplicates,
            })?;
            if let Some((name, _)) = field {
                node.fields.insert(name, value);
            }
        }
        Ok(node)
    }
}
impl Seed<'_> {
    fn numeric<E: de::Error>(self, value: String) -> Result<Node, E> {
        Ok(Node {
            kind: Kind::Number,
            number: (self.context == Context::Number).then_some(value),
            ..Node::default()
        })
    }
}
struct Key<'a>(Cow<'a, str>);
impl<'de> Deserialize<'de> for Key<'de> {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KeyVisitor;
        impl<'de> Visitor<'de> for KeyVisitor {
            type Value = Key<'de>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an object key")
            }
            fn visit_borrowed_str<E: de::Error>(self, value: &'de str) -> Result<Self::Value, E> {
                Ok(Key(Cow::Borrowed(value)))
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(Key(Cow::Owned(value.to_owned())))
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(Key(Cow::Owned(value)))
            }
        }
        deserializer.deserialize_str(KeyVisitor)
    }
}

#[cfg(test)]
#[path = "request_facts_tests.rs"]
mod tests;
