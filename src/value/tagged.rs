use crate::value::de::{MapDeserializer, MapRefDeserializer, SeqDeserializer, SeqRefDeserializer};
use crate::value::Value;
use crate::Error;
use serde::de::value::{BorrowedStrDeserializer, StrDeserializer};
use serde::de::{
    Deserialize, DeserializeSeed, Deserializer, EnumAccess, Error as _, VariantAccess, Visitor,
};
use serde::ser::{Serialize, SerializeMap, Serializer};
use std::cmp::Ordering;
use std::fmt::{self, Debug};
use std::hash::{Hash, Hasher};

/// A representation of YAML's `!Tag` syntax, used for enums.
///
/// Refer to the example code on [`TaggedValue`] for an example of deserializing
/// tagged values.
#[derive(Clone)]
pub struct Tag {
    pub(crate) string: String,
}

/// A `Tag` + `Value` representing a tagged YAML scalar, sequence, or mapping.
///
/// ```
/// use serde_yaml::value::TaggedValue;
/// use std::collections::BTreeMap;
///
/// let yaml = "
///     scalar: !Thing x
///     sequence_flow: !Thing [first]
///     sequence_block: !Thing
///       - first
///     mapping_flow: !Thing {k: v}
///     mapping_block: !Thing
///       k: v
/// ";
///
/// let data: BTreeMap<String, TaggedValue> = serde_yaml::from_str(yaml).unwrap();
/// assert!(data["scalar"].tag == "Thing");
/// assert!(data["sequence_flow"].tag == "Thing");
/// assert!(data["sequence_block"].tag == "Thing");
/// assert!(data["mapping_flow"].tag == "Thing");
/// assert!(data["mapping_block"].tag == "Thing");
///
/// // The leading '!' in tags are not significant. The following is also true.
/// assert!(data["scalar"].tag == "!Thing");
/// ```
#[derive(Clone, PartialEq, PartialOrd, Hash, Debug)]
pub struct TaggedValue {
    #[allow(missing_docs)]
    pub tag: Tag,
    #[allow(missing_docs)]
    pub value: Value,
}

impl Tag {
    /// Create tag.
    ///
    /// The leading '!' is not significant. It may be provided, but does not
    /// have to be. The following are equivalent:
    ///
    /// ```
    /// use serde_yaml::value::Tag;
    ///
    /// assert_eq!(Tag::new("!Thing"), Tag::new("Thing"));
    ///
    /// let tag = Tag::new("Thing");
    /// assert!(tag == "Thing");
    /// assert!(tag == "!Thing");
    ///
    /// let tag = Tag::new("!Thing");
    /// assert!(tag == "Thing");
    /// assert!(tag == "!Thing");
    /// ```
    ///
    /// Such a tag would serialize to `!Thing` in YAML regardless of whether a
    /// '!' was included in the call to `Tag::new`.
    pub fn new(string: impl Into<String>) -> Self {
        Tag {
            string: string.into(),
        }
    }
}

impl Value {
    pub(crate) fn untag(self) -> Self {
        let mut cur = self;
        while let Value::Tagged(tagged) = cur {
            cur = tagged.value;
        }
        cur
    }

    pub(crate) fn untag_ref(&self) -> &Self {
        let mut cur = self;
        while let Value::Tagged(tagged) = cur {
            cur = &tagged.value;
        }
        cur
    }

    pub(crate) fn untag_mut(&mut self) -> &mut Self {
        let mut cur = self;
        while let Value::Tagged(tagged) = cur {
            cur = &mut tagged.value;
        }
        cur
    }
}

pub(crate) fn nobang(maybe_banged: &str) -> &str {
    maybe_banged.strip_prefix('!').unwrap_or(maybe_banged)
}

impl Eq for Tag {}

impl PartialEq for Tag {
    fn eq(&self, other: &Tag) -> bool {
        PartialEq::eq(nobang(&self.string), nobang(&other.string))
    }
}

impl<T> PartialEq<T> for Tag
where
    T: ?Sized + AsRef<str>,
{
    fn eq(&self, other: &T) -> bool {
        PartialEq::eq(nobang(&self.string), nobang(other.as_ref()))
    }
}

impl Ord for Tag {
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(nobang(&self.string), nobang(&other.string))
    }
}

impl PartialOrd for Tag {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        PartialOrd::partial_cmp(nobang(&self.string), nobang(&other.string))
    }
}

impl Hash for Tag {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        nobang(&self.string).hash(hasher);
    }
}

impl Debug for Tag {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "!{}", nobang(&self.string))
    }
}

impl Serialize for TaggedValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(&format_args!("!{}", nobang(&self.tag.string)), &self.value)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for TaggedValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TaggedValueVisitor;

        impl<'de> Visitor<'de> for TaggedValueVisitor {
            type Value = TaggedValue;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a YAML value with a !Tag")
            }

            fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
            where
                A: EnumAccess<'de>,
            {
                let (tag, contents) = data.variant::<String>()?;
                let tag = Tag::new(tag);
                let value = contents.newtype_variant()?;
                Ok(TaggedValue { tag, value })
            }
        }

        deserializer.deserialize_any(TaggedValueVisitor)
    }
}

impl<'de> EnumAccess<'de> for TaggedValue {
    type Error = Error;
    type Variant = Value;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Error>
    where
        V: DeserializeSeed<'de>,
    {
        let tag = StrDeserializer::<Error>::new(nobang(&self.tag.string));
        let value = seed.deserialize(tag)?;
        Ok((value, self.value))
    }
}

impl<'de> VariantAccess<'de> for Value {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        Deserialize::deserialize(self)
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Error>
    where
        T: DeserializeSeed<'de>,
    {
        seed.deserialize(self)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        if let Value::Sequence(v) = self {
            Deserializer::deserialize_any(SeqDeserializer::new(v), visitor)
        } else {
            Err(Error::invalid_type(self.unexpected(), &"tuple variant"))
        }
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        if let Value::Mapping(v) = self {
            Deserializer::deserialize_any(MapDeserializer::new(v), visitor)
        } else {
            Err(Error::invalid_type(self.unexpected(), &"struct variant"))
        }
    }
}

impl<'de> EnumAccess<'de> for &'de TaggedValue {
    type Error = Error;
    type Variant = &'de Value;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Error>
    where
        V: DeserializeSeed<'de>,
    {
        let tag = BorrowedStrDeserializer::<Error>::new(nobang(&self.tag.string));
        let value = seed.deserialize(tag)?;
        Ok((value, &self.value))
    }
}

impl<'de> VariantAccess<'de> for &'de Value {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        Deserialize::deserialize(self)
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Error>
    where
        T: DeserializeSeed<'de>,
    {
        seed.deserialize(self)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        if let Value::Sequence(v) = self {
            Deserializer::deserialize_any(SeqRefDeserializer::new(v), visitor)
        } else {
            Err(Error::invalid_type(self.unexpected(), &"tuple variant"))
        }
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        if let Value::Mapping(v) = self {
            Deserializer::deserialize_any(MapRefDeserializer::new(v), visitor)
        } else {
            Err(Error::invalid_type(self.unexpected(), &"struct variant"))
        }
    }
}
