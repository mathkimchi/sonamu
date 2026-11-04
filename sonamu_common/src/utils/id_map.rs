//! REVIEW: is this a good idea?
//! Even if I get the derives to work, this might be a bit wonky

use std::{collections::BTreeMap, marker::PhantomData};
use uuid::Uuid;

// #[derive(serde::Deserialize)]
pub struct Id<Item> {
    uuid: Uuid,
    /// Compiler will complain if I don't have this here
    // #[serde(skip)]
    phantom_data: PhantomData<Item>,
}
impl<Item> Id<Item> {
    #[must_use]
    pub fn generate() -> Self {
        Uuid::new_v4().into()
    }

    /// Like re-casting, just change the types which is fine
    /// for this struct since its all phantom data anyways
    #[must_use]
    pub fn transmute<NewItem>(self) -> Id<NewItem> {
        unsafe { std::mem::transmute(self) }
    }
}
/// Derive doesn't understand phantomdata
mod id_derive_impls {
    use super::Id;
    use std::marker::PhantomData;
    use uuid::Uuid;

    impl<Item> From<Uuid> for Id<Item> {
        fn from(value: Uuid) -> Self {
            Self {
                uuid: value,
                phantom_data: PhantomData,
            }
        }
    }
    impl<Item> From<Id<Item>> for Uuid {
        fn from(value: Id<Item>) -> Self {
            value.uuid
        }
    }
    impl<Item> Copy for Id<Item> {}
    impl<Item> Clone for Id<Item> {
        fn clone(&self) -> Self {
            *self
        }
    }
    impl<Item> PartialEq for Id<Item> {
        fn eq(&self, other: &Self) -> bool {
            self.uuid.eq(&other.uuid)
        }
    }
    impl<Item> Eq for Id<Item> {}
    impl<Item> PartialOrd for Id<Item> {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl<Item> Ord for Id<Item> {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.uuid.cmp(&other.uuid)
        }
    }
    impl<Item> std::fmt::Debug for Id<Item> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Id").field("uuid", &self.uuid).finish()
        }
    }

    /// Serialize as a str for the sake of map keys, I think the phantom data messes with it
    impl<Item> serde::Serialize for Id<Item> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.uuid.serialize(serializer)
        }
    }
    impl<'de, Item> serde::Deserialize<'de> for Id<Item> {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct IdVisitor<Item> {
                phantom_data: PhantomData<Item>,
            }

            impl<Item> serde::de::Visitor<'_> for IdVisitor<Item> {
                type Value = Id<Item>;
                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    write!(formatter, "a UUID string")
                }
                fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Id<Item>, E> {
                    Ok(Id {
                        uuid: value
                            .parse::<Uuid>()
                            .map_err(|e| E::custom(format!("Failed to parse UUID for Id: {e}")))?,
                        phantom_data: PhantomData,
                    })
                }
                fn visit_bytes<E: serde::de::Error>(self, value: &[u8]) -> Result<Id<Item>, E> {
                    Ok(Id {
                        uuid: Uuid::from_slice(value)
                            .map_err(|e| E::custom(format!("Failed to parse UUID for Id: {e}")))?,
                        phantom_data: PhantomData,
                    })
                }
            }

            deserializer.deserialize_str(IdVisitor::<Item> {
                phantom_data: PhantomData,
            })
        }
    }
}

pub type IdMap<Item> = BTreeMap<Id<Item>, Item>;
