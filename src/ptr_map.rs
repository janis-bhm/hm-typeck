use std::any::TypeId;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

/// A pointer used as an identity key.
pub struct Ptr<T: ?Sized>(pub *const T, pub TypeId);

impl<T: ?Sized> Ptr<T> {
    /// Returns the address of the pointed-to value, without pointer metadata.
    pub fn addr(self) -> usize {
        self.0 as *const () as usize
    }

    /// Erases the pointee type while preserving its address identity.
    pub fn erase(self) -> Ptr<()> {
        Ptr(self.0 as *const (), self.1)
    }
}

impl<T: ?Sized> Copy for Ptr<T> {}

impl<T: ?Sized> Clone for Ptr<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> fmt::Debug for Ptr<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Ptr").field(&self.addr()).finish()
    }
}

impl<T: ?Sized> PartialEq for Ptr<T> {
    fn eq(&self, other: &Self) -> bool {
        self.addr() == other.addr() && self.1 == other.1
    }
}

impl<T: ?Sized> Eq for Ptr<T> {}

impl<T: ?Sized> Hash for Ptr<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.addr().hash(state);
        self.1.hash(state);
    }
}

impl<T: ?Sized + 'static> From<&T> for Ptr<T> {
    fn from(value: &T) -> Self {
        Self(value as *const T, TypeId::of::<T>())
    }
}

/// A map keyed by the address and pointee type identity of borrowed values.
///
/// Values with equal contents but different addresses are distinct keys.
///
/// Note: the pointee type is part of the key because a compiler may co-locate a
/// field at offset zero of its parent. For example, enum payload niching can put
/// `Stmt::Let`'s `init` at the same address as the enclosing `Stmt`, so address
/// alone could alias distinct AST nodes.
///
/// Do not insert keys whose referents may be dropped or moved while the map is
/// alive; the borrow lifetime enforces this for values inserted through
/// [`PtrMap::insert`].
pub struct PtrMap<'a, V> {
    map: HashMap<Ptr<()>, V>,
    _borrow: PhantomData<&'a ()>,
}

impl<'a, V> PtrMap<'a, V> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            _borrow: PhantomData,
        }
    }

    pub fn insert<T: ?Sized + 'static>(&mut self, key: &'a T, value: V) -> Option<V> {
        self.map.insert(Ptr::from(key).erase(), value)
    }

    pub fn get<T: ?Sized + 'static>(&self, key: &T) -> Option<&V> {
        self.map.get(&Ptr::from(key).erase())
    }

    pub fn get_mut<T: ?Sized + 'static>(&mut self, key: &T) -> Option<&mut V> {
        self.map.get_mut(&Ptr::from(key).erase())
    }

    pub fn remove<T: ?Sized + 'static>(&mut self, key: &T) -> Option<V> {
        self.map.remove(&Ptr::from(key).erase())
    }

    pub fn contains_key<T: ?Sized + 'static>(&self, key: &T) -> bool {
        self.map.contains_key(&Ptr::from(key).erase())
    }
}

impl<V> Deref for PtrMap<'_, V> {
    type Target = HashMap<Ptr<()>, V>;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl<V> DerefMut for PtrMap<'_, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.map
    }
}

impl<V> Default for PtrMap<'_, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: fmt::Debug> fmt::Debug for PtrMap<'_, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.map.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::{Expr, PtrMap, Stmt};

    #[test]
    fn keys_are_expression_address_identities() {
        let e1 = Expr::int(1);
        let e2 = Expr::int(1);
        let mut map = PtrMap::new();

        map.insert(&e1, "first");

        assert_eq!(map.get(&e1), Some(&"first"));
        assert_eq!(map.get(&e2), None);
        assert!(!map.contains_key(&e2));
        assert_eq!(map.len(), 1);
        assert_eq!(map.iter().count(), 1);

        *map.get_mut(&e1).expect("e1 was inserted") = "updated";
        assert_eq!(map.get(&e1), Some(&"updated"));

        // The borrowed nodes enclose the map's use, as required by its type.
        assert_eq!(map.remove(&e1), Some("updated"));
        assert!(map.is_empty());
    }

    #[test]
    fn supports_unsized_string_keys() {
        let first = String::from("same");
        let second = String::from("same");
        let expr = Expr::int(2);
        let mut map = PtrMap::<i32>::new();

        map.insert(first.as_str(), 1);
        map.insert(&expr, 2);

        assert_eq!(map.get(first.as_str()), Some(&1));
        assert_eq!(map.get(second.as_str()), None);
        assert_eq!(map.get(&expr), Some(&2));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn supports_heterogeneous_key_types() {
        let expr = Expr::int(1);
        let stmt = Stmt::Expr(Expr::int(2));
        let mut map = PtrMap::<&str>::new();

        map.insert(&expr, "expression");
        map.insert(&stmt, "statement");

        assert_eq!(map.get(&expr), Some(&"expression"));
        assert_eq!(map.get(&stmt), Some(&"statement"));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn distinguishes_overlapping_parent_and_field_addresses() {
        let stmt = Stmt::Let {
            name: std::rc::Rc::new("x".to_owned()),
            ty: None,
            init: Expr::int(1),
        };
        let Stmt::Let { init, .. } = &stmt else {
            unreachable!();
        };
        assert_eq!(&stmt as *const Stmt as usize, init as *const Expr as usize);

        let mut map = PtrMap::new();
        map.insert(&stmt, "statement");
        map.insert(init, "initializer");

        assert_eq!(map.get(&stmt), Some(&"statement"));
        assert_eq!(map.get(init), Some(&"initializer"));
        assert_eq!(map.len(), 2);
    }
}
