use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

/// A pointer used as an identity key.
pub struct Ptr<T: ?Sized>(pub *const T);

impl<T: ?Sized> Ptr<T> {
    /// Returns the address of the pointed-to value, without pointer metadata.
    pub fn addr(self) -> usize {
        self.0 as *const () as usize
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
        self.addr() == other.addr()
    }
}

impl<T: ?Sized> Eq for Ptr<T> {}

impl<T: ?Sized> Hash for Ptr<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.addr().hash(state);
    }
}

impl<T: ?Sized> From<&T> for Ptr<T> {
    fn from(value: &T) -> Self {
        Self(value as *const T)
    }
}

/// A map keyed by the address identity of borrowed values.
pub struct PtrMap<'a, T: ?Sized, V> {
    map: HashMap<Ptr<T>, V>,
    _borrow: PhantomData<&'a T>,
}

impl<'a, T: ?Sized, V> PtrMap<'a, T, V> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            _borrow: PhantomData,
        }
    }

    pub fn insert(&mut self, key: &'a T, value: V) -> Option<V> {
        self.map.insert(Ptr::from(key), value)
    }

    pub fn get(&self, key: &T) -> Option<&V> {
        self.map.get(&Ptr::from(key))
    }

    pub fn get_mut(&mut self, key: &T) -> Option<&mut V> {
        self.map.get_mut(&Ptr::from(key))
    }

    pub fn remove(&mut self, key: &T) -> Option<V> {
        self.map.remove(&Ptr::from(key))
    }

    pub fn contains_key(&self, key: &T) -> bool {
        self.map.contains_key(&Ptr::from(key))
    }
}

impl<T: ?Sized, V> Deref for PtrMap<'_, T, V> {
    type Target = HashMap<Ptr<T>, V>;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl<T: ?Sized, V> DerefMut for PtrMap<'_, T, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.map
    }
}

impl<T: ?Sized, V> Default for PtrMap<'_, T, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ?Sized, V: fmt::Debug> fmt::Debug for PtrMap<'_, T, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.map.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::{Expr, PtrMap};

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
        let mut map = PtrMap::<str, _>::new();

        map.insert(first.as_str(), 1);

        assert_eq!(map.get(first.as_str()), Some(&1));
        assert_eq!(map.get(second.as_str()), None);
    }
}
