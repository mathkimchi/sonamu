use super::tree_node_path::{TraversableTree, TreeNodePath};
use crate::utils::id_map::Id;
use std::collections::{BTreeMap, BTreeSet};

/// Technically, children are ordered
struct Node<T> {
    children: Vec<Id<T>>,
    parent: Option<Id<T>>,
}

/// Only stores hierarchy, no items.
/// TODO: Look into `slotmap`?
pub struct IdTree<T> {
    root_id: Id<T>,
    nodes: BTreeMap<Id<T>, Node<T>>,
}
impl<T> IdTree<T> {
    #[must_use]
    pub fn new(root_id: Id<T>) -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(root_id, Node::default());
        Self { root_id, nodes }
    }

    pub fn add_child(&mut self, parent_id: Id<T>, child_id: Id<T>) -> Option<()> {
        // add child to `nodes`
        self.nodes.insert(
            child_id,
            Node {
                children: Vec::new(),
                parent: Some(parent_id),
            },
        );
        // register child under parent
        self.nodes.get_mut(&parent_id)?.children.push(child_id);

        Some(())
    }

    /// generates a new id and adds it
    pub fn create_child(&mut self, parent_id: Id<T>) -> Option<Id<T>> {
        let child_id = Id::generate();
        self.add_child(parent_id, child_id);
        Some(child_id)
    }

    /// recursively removes id and all its posterity, and creates a new id tree with the id being root
    ///
    /// If this is root, then don't change anything
    pub fn pluck(&mut self, new_root_id: &Id<T>) -> Option<Self> {
        // disconnect from parent
        let root_parent_id = self.nodes.get(new_root_id).unwrap().parent?;
        self.nodes
            .get_mut(&root_parent_id)
            .unwrap()
            .children
            .retain(|id| id != new_root_id);
        self.nodes.get_mut(new_root_id).unwrap().parent = None;

        // move this and all its children into a new tree
        let nodes = {
            let mut nodes = BTreeMap::new();

            // bfs search
            let mut unprocessed_ids = BTreeSet::from_iter([*new_root_id]);
            while !unprocessed_ids.is_empty() {
                let mut new_unprocessed_ids = BTreeSet::new(); // children of the previously unprocessed
                for unprocessed_id in &unprocessed_ids {
                    // process by taking it out of old tree, noting its children, and putting in the new tree
                    let unprocessed_node = self.nodes.remove(unprocessed_id).unwrap();
                    for child in unprocessed_node.children.clone() {
                        new_unprocessed_ids.insert(child);
                    }
                    nodes.insert(*unprocessed_id, unprocessed_node);
                }
                unprocessed_ids = new_unprocessed_ids;
            }

            nodes
        };

        Some(Self {
            root_id: *new_root_id,
            nodes,
        })
    }

    /// similar logic to `add_child`
    ///
    /// meant to be used with pluck, for pluck and place
    pub fn place_as_children(&mut self, mut sub_tree: Self, sub_tree_parent_id: Id<T>) {
        // connect root of subtree with the parent
        self.nodes
            .get_mut(&sub_tree_parent_id)
            .unwrap()
            .children
            .push(sub_tree.root_id);
        sub_tree.nodes.get_mut(&sub_tree.root_id).unwrap().parent = Some(sub_tree_parent_id);

        // add subtree's nodes into self's nodes
        self.nodes.append(&mut sub_tree.nodes);
    }

    #[must_use]
    pub const fn get_root_id(&self) -> Id<T> {
        self.root_id
    }

    /// climb downwards
    #[must_use]
    pub fn get_id_from_path(&self, path: &TreeNodePath) -> Option<Id<T>> {
        let mut node_id = self.root_id;

        for index in &path.0 {
            node_id = *self.nodes.get(&node_id)?.children.get(*index)?;
        }

        Some(node_id)
    }

    #[must_use]
    pub fn get_children(&self, parent_id: &Id<T>) -> &Vec<Id<T>> {
        &self.nodes.get(parent_id).unwrap().children
    }

    /// climb upwards
    #[must_use]
    pub fn get_path(&self, id: Id<T>) -> Option<TreeNodePath> {
        let mut path_vec = Vec::new();
        let mut node_id = id;

        while let Some(parent_id) = self.nodes.get(&node_id).unwrap().parent {
            let child_index = self
                .nodes
                .get(&parent_id)
                .unwrap()
                .children
                .iter()
                .position(|id| id == &node_id)
                .unwrap();
            path_vec.push(child_index);

            node_id = parent_id;
        }

        path_vec.reverse();

        Some(path_vec.into())
    }

    /// NOTE: will not detect long term cycles
    fn locally_consistent(&self, id: Id<T>) -> Result<(), String> {
        let node = &self.nodes[&id];

        // parent notes this as child
        if let Some(parent_id) = &node.parent {
            if !self.nodes[parent_id].children.contains(&id) {
                return Err(format!(
                    "Node {:?}'s parent {:?} does not have node as child out of children {:?}",
                    id, parent_id, self.nodes[parent_id].children
                ));
            }
            if self.root_id == id {
                return Err(format!(
                    "Node {id:?} has parent {parent_id:?} but is also root"
                ));
            }
            if parent_id == &id {
                return Err(format!("Node {id:?} is its own parent"));
            }
        } else {
            // root pointer serves as pointer if no parent
            if self.root_id != id {
                return Err(format!(
                    "Node {:?} has no parent but is not root {:?}",
                    id, self.root_id
                ));
            }
        }

        // children notes this as parent
        for child_id in &node.children {
            if self.nodes[child_id].parent != Some(id) {
                return Err(format!(
                    "Node {:?}'s child {:?}'s parent {:?} is not node'",
                    id, child_id, self.nodes[child_id].parent
                ));
            }
        }

        Ok(())
    }

    fn update_parent_connections(&mut self, parent_id: &Id<T>) {
        dbg!("Hey");
        for child_id in self.get_children(parent_id).clone() {
            dbg!("Hi");
            self.nodes.get_mut(&child_id).unwrap().parent = Some(*parent_id);
        }
    }

    pub fn swap_ids(&mut self, ids_to_swap: [Id<T>; 2]) {
        // swap for parents, if root, then the "parent" is root_id
        unsafe {
            // using unsafe raw pointers is the only "elegant" way I could think of doing this
            // TODO: I realize that I can just set it equal to the other, ^ is wrong

            // represents pointers to the ids from the parent's perspective
            let ptrs: [*mut Id<T>; 2] = ids_to_swap.map(|id| {
                if let Some(parent_id) = self.nodes[&id].parent {
                    std::ptr::from_mut::<Id<T>>(
                        self.nodes
                            .get_mut(&parent_id)
                            .unwrap()
                            .children
                            .iter_mut()
                            .find(|other| other == &&id)
                            .unwrap(),
                    )
                } else {
                    &raw mut self.root_id
                }
            });

            std::ptr::swap(ptrs[0], ptrs[1]);
        }

        // swap children
        unsafe {
            // using unsafe raw pointers is the only "elegant" way I could think of doing this
            // TODO: like `swap for parents` and `swap parents`, there is actually a better way

            // represents pointers to the ids from the parent's perspective
            let children: [*mut Vec<Id<T>>; 2] =
                ids_to_swap.map(|id| &raw mut self.nodes.get_mut(&id).unwrap().children);

            std::ptr::swap(children[0], children[1]);
        }

        // now, all the children connections should be correct
        // update all parent connections to match all children connections
        {
            // pretty redundant, but should work
            self.nodes.get_mut(&self.root_id).unwrap().parent = None;
            for id in &ids_to_swap {
                if let Some(parent_id) = self.nodes.get_mut(id).unwrap().parent {
                    self.update_parent_connections(&parent_id);
                }
            }
            for id in &ids_to_swap {
                self.update_parent_connections(id);
            }

            // for id in self.nodes.clone().keys() {
            //     self.update_parent_connections(id);
            // }
        }

        // // swap node values
        // unsafe {
        //     // using unsafe raw pointers is the only "elegant" way I could think of doing this
        //     // TODO: like `swap for parents` and `swap parents`, there is actually a better way

        //     // represents pointers to the ids from the parent's perspective
        //     let node: [*mut Node<T>; 2] =
        //         ids_to_swap.map(|id| self.nodes.get_mut(&id).unwrap() as *mut Node<T>);

        //     std::ptr::swap(node[0], node[1]);
        // }

        // // swap for children
        // for id in ids_to_swap {
        //     let other_id = ids_to_swap.iter().find(|o| o == &&id).unwrap();
        //     for child in self.nodes.get(&id).unwrap().children.clone() {
        //         self.nodes.get_mut(&child).unwrap().parent = Some(*other_id);
        //     }
        // }

        // // swap parents
        // unsafe {
        //     // using unsafe raw pointers is the only "elegant" way I could think of doing this
        //     // TODO: like `swap for parents`, there is actually a better way

        //     // represents pointers to the ids from the parent's perspective
        //     let parents: [*mut Option<Id<T>>; 2] = ids_to_swap
        //         .map(|id| &mut self.nodes.get_mut(&id).unwrap().parent as *mut Option<Id<T>>);

        //     std::ptr::swap(parents[0], parents[1]);
        // }

        dbg!(&self);
        self.locally_consistent(ids_to_swap[0]).unwrap();
        self.locally_consistent(ids_to_swap[1]).unwrap();
    }

    /// Like re-casting, just change the types which is fine
    /// for this struct since its all phantom data anyways
    #[must_use]
    pub fn transmute<NewItem>(self) -> IdTree<NewItem> {
        unsafe { std::mem::transmute(self) }
    }
}
impl<T> TraversableTree for IdTree<T> {
    fn exists_at(&self, path: &TreeNodePath) -> bool {
        self.get_id_from_path(path).is_some()
    }
}

/// for some of the derive impls, I just took the type restrictions out of the derive macros
#[allow(clippy::pedantic)]
mod derive_macro_impls {
    use super::*;

    impl<T> Default for Node<T> {
        fn default() -> Self {
            Self {
                children: Default::default(),
                parent: Default::default(),
            }
        }
    }
    impl<T> Clone for Node<T> {
        fn clone(&self) -> Self {
            Self {
                children: self.children.clone(),
                parent: self.parent,
            }
        }
    }
    impl<T> serde::Serialize for Node<T> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            use serde::ser::SerializeStruct;
            let mut serialized = serializer.serialize_struct("Node", 2)?;
            serialized.serialize_field("children", &self.children)?;
            serialized.serialize_field("parent", &self.parent)?;
            serialized.end()
        }
    }
    /// Based off: https://serde.rs/deserialize-struct.html and by expanding the serialize macro
    impl<'de, T> serde::Deserialize<'de> for Node<T> {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            enum Field {
                Children,
                Parent,
            }
            impl<'de> serde::Deserialize<'de> for Field {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    struct FieldVisitor;

                    impl serde::de::Visitor<'_> for FieldVisitor {
                        type Value = Field;

                        fn expecting(
                            &self,
                            formatter: &mut std::fmt::Formatter,
                        ) -> std::fmt::Result {
                            formatter.write_str("`children` or `parent`")
                        }

                        fn visit_str<E>(self, value: &str) -> Result<Field, E>
                        where
                            E: serde::de::Error,
                        {
                            match value {
                                "children" => Ok(Field::Children),
                                "parent" => Ok(Field::Parent),
                                _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                            }
                        }
                    }

                    deserializer.deserialize_identifier(FieldVisitor)
                }
            }

            struct IdVisitor<T> {
                phantom_data: std::marker::PhantomData<T>,
            }

            impl<'de, T> serde::de::Visitor<'de> for IdVisitor<T> {
                type Value = Node<T>;
                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    write!(formatter, "struct Node")
                }
                fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
                where
                    A: serde::de::MapAccess<'de>,
                {
                    let mut children: Option<Vec<Id<T>>> = None;
                    let mut parent: Option<Option<Id<T>>> = None;
                    while let Some(key) = map.next_key()? {
                        match key {
                            Field::Children => {
                                if children.is_some() {
                                    return Err(serde::de::Error::duplicate_field("children"));
                                }
                                children = Some(map.next_value()?);
                            }
                            Field::Parent => {
                                if parent.is_some() {
                                    return Err(serde::de::Error::duplicate_field("parent"));
                                }
                                parent = Some(map.next_value()?);
                            }
                        }
                    }

                    Ok(Node {
                        children: children.ok_or(serde::de::Error::missing_field("children"))?,
                        parent: parent.ok_or(serde::de::Error::missing_field("parent"))?,
                    })
                }
            }

            const FIELDS: &[&str] = &["children", "parent"];
            deserializer.deserialize_struct(
                "Id",
                FIELDS,
                IdVisitor::<T> {
                    phantom_data: std::marker::PhantomData,
                },
            )
        }
    }
    impl<T> core::fmt::Debug for Node<T> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            let Self { children, parent } = self;
            f.debug_struct("Node")
                .field("children", &children)
                .field("parent", &parent)
                .finish()
        }
    }

    impl<T> Default for IdTree<T> {
        fn default() -> Self {
            Self::new(Id::generate())
        }
    }
    impl<T> serde::Serialize for IdTree<T> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            use serde::ser::SerializeStruct;
            let mut serialized = serializer.serialize_struct("Node", 2)?;
            serialized.serialize_field("root_id", &self.root_id)?;
            serialized.serialize_field("nodes", &self.nodes)?;
            serialized.end()
        }
    }
    impl<'de, T> serde::Deserialize<'de> for IdTree<T> {
        fn deserialize<__D>(__deserializer: __D) -> Result<Self, __D::Error>
        where
            __D: serde::Deserializer<'de>,
        {
            #[allow(non_camel_case_types)]
            #[doc(hidden)]
            enum __Field {
                __field0,
                __field1,
                __ignore,
            }
            #[doc(hidden)]
            struct __FieldVisitor;

            impl serde::de::Visitor<'_> for __FieldVisitor {
                type Value = __Field;
                fn expecting(
                    &self,
                    __formatter: &mut serde::__private228::Formatter,
                ) -> serde::__private228::fmt::Result {
                    serde::__private228::Formatter::write_str(__formatter, "field identifier")
                }
                fn visit_u64<__E>(self, __value: u64) -> Result<Self::Value, __E>
                where
                    __E: serde::de::Error,
                {
                    match __value {
                        0u64 => Ok(__Field::__field0),
                        1u64 => Ok(__Field::__field1),
                        _ => Ok(__Field::__ignore),
                    }
                }
                fn visit_str<__E>(self, __value: &str) -> Result<Self::Value, __E>
                where
                    __E: serde::de::Error,
                {
                    match __value {
                        "root_id" => Ok(__Field::__field0),
                        "nodes" => Ok(__Field::__field1),
                        _ => Ok(__Field::__ignore),
                    }
                }
                fn visit_bytes<__E>(self, __value: &[u8]) -> Result<Self::Value, __E>
                where
                    __E: serde::de::Error,
                {
                    match __value {
                        b"root_id" => Ok(__Field::__field0),
                        b"nodes" => Ok(__Field::__field1),
                        _ => Ok(__Field::__ignore),
                    }
                }
            }
            impl<'de> serde::Deserialize<'de> for __Field {
                #[inline]
                fn deserialize<__D>(__deserializer: __D) -> Result<Self, __D::Error>
                where
                    __D: serde::Deserializer<'de>,
                {
                    serde::Deserializer::deserialize_identifier(__deserializer, __FieldVisitor)
                }
            }
            #[doc(hidden)]
            struct __Visitor<'de, T> {
                marker: serde::__private228::PhantomData<IdTree<T>>,
                lifetime: serde::__private228::PhantomData<&'de ()>,
            }
            impl<'de, T> serde::de::Visitor<'de> for __Visitor<'de, T> {
                type Value = IdTree<T>;
                fn expecting(
                    &self,
                    __formatter: &mut serde::__private228::Formatter,
                ) -> serde::__private228::fmt::Result {
                    serde::__private228::Formatter::write_str(__formatter, "struct IdTree")
                }
                #[inline]
                fn visit_seq<__A>(self, mut __seq: __A) -> Result<Self::Value, __A::Error>
                where
                    __A: serde::de::SeqAccess<'de>,
                {
                    let __field0 = match serde::de::SeqAccess::next_element::<Id<T>>(&mut __seq)? {
                        Some(__value) => __value,
                        None => {
                            return Err(serde::de::Error::invalid_length(
                                0usize,
                                &"struct IdTree with 2 elements",
                            ));
                        }
                    };
                    let __field1 = match serde::de::SeqAccess::next_element::<
                        BTreeMap<Id<T>, Node<T>>,
                    >(&mut __seq)?
                    {
                        Some(__value) => __value,
                        None => {
                            return Err(serde::de::Error::invalid_length(
                                1usize,
                                &"struct IdTree with 2 elements",
                            ));
                        }
                    };
                    Ok(IdTree {
                        root_id: __field0,
                        nodes: __field1,
                    })
                }
                #[inline]
                fn visit_map<__A>(self, mut __map: __A) -> Result<Self::Value, __A::Error>
                where
                    __A: serde::de::MapAccess<'de>,
                {
                    let mut __field0: Option<Id<T>> = None;
                    let mut __field1: Option<BTreeMap<Id<T>, Node<T>>> = None;
                    while let Some(__key) = serde::de::MapAccess::next_key::<__Field>(&mut __map)? {
                        match __key {
                            __Field::__field0 => {
                                if Option::is_some(&__field0) {
                                    return Err(<__A::Error as serde::de::Error>::duplicate_field(
                                        "root_id",
                                    ));
                                }
                                __field0 =
                                    Some(serde::de::MapAccess::next_value::<Id<T>>(&mut __map)?);
                            }
                            __Field::__field1 => {
                                if Option::is_some(&__field1) {
                                    return Err(<__A::Error as serde::de::Error>::duplicate_field(
                                        "nodes",
                                    ));
                                }
                                __field1 = Some(serde::de::MapAccess::next_value::<
                                    BTreeMap<Id<T>, Node<T>>,
                                >(&mut __map)?);
                            }
                            _ => {
                                let _ = serde::de::MapAccess::next_value::<serde::de::IgnoredAny>(
                                    &mut __map,
                                )?;
                            }
                        }
                    }
                    let __field0 = match __field0 {
                        Some(__field0) => __field0,
                        None => serde::__private228::de::missing_field("root_id")?,
                    };
                    let __field1 = match __field1 {
                        Some(__field1) => __field1,
                        None => serde::__private228::de::missing_field("nodes")?,
                    };
                    Ok(IdTree {
                        root_id: __field0,
                        nodes: __field1,
                    })
                }
            }
            #[doc(hidden)]
            const FIELDS: &[&str] = &["root_id", "nodes"];
            serde::Deserializer::deserialize_struct(
                __deserializer,
                "IdTree",
                FIELDS,
                __Visitor {
                    marker: serde::__private228::PhantomData::<Self>,
                    lifetime: serde::__private228::PhantomData,
                },
            )
        }
    }
    impl<T> Clone for IdTree<T> {
        fn clone(&self) -> Self {
            let Self { root_id, nodes } = self;
            Self {
                root_id: *root_id,
                nodes: nodes.clone(),
            }
        }
    }
    impl<T> core::fmt::Debug for IdTree<T> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            let Self { root_id, nodes } = self;
            f.debug_struct("IdTree")
                .field("root_id", &root_id)
                .field("nodes", &nodes)
                .finish()
        }
    }
}
