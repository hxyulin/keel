//! Values keyed by an input object's own section or symbol index. Those
//! indices are dense, so a vector replaces an ordered map; iteration stays in
//! ascending key order.

#[derive(Clone, Debug)]
pub(super) struct Slots<T> {
    slots: Vec<Option<T>>,
    len: usize,
}

impl<T> Default for Slots<T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            len: 0,
        }
    }
}

impl<T> Slots<T> {
    pub fn len(&self) -> usize {
        self.len
    }
    /// One past the largest key ever inserted.
    pub fn bound(&self) -> usize {
        self.slots.len()
    }
    /// The largest key with a value.
    pub fn last_key(&self) -> Option<usize> {
        self.slots.iter().rposition(Option::is_some)
    }
    pub fn get(&self, key: &usize) -> Option<&T> {
        self.slots.get(*key).and_then(Option::as_ref)
    }
    #[cfg(test)]
    pub fn get_mut(&mut self, key: &usize) -> Option<&mut T> {
        self.slots.get_mut(*key).and_then(Option::as_mut)
    }
    pub fn contains_key(&self, key: &usize) -> bool {
        self.get(key).is_some()
    }
    pub fn insert(&mut self, key: usize, value: T) -> Option<T> {
        if key >= self.slots.len() {
            self.slots.resize_with(key + 1, || None);
        }
        let previous = self.slots[key].replace(value);
        if previous.is_none() {
            self.len += 1;
        }
        previous
    }
    pub fn remove(&mut self, key: &usize) -> Option<T> {
        let previous = self.slots.get_mut(*key)?.take();
        if previous.is_some() {
            self.len -= 1;
        }
        previous
    }
    pub fn iter(&self) -> impl Iterator<Item = (usize, &T)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(key, value)| Some((key, value.as_ref()?)))
    }
    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.slots.iter().flatten()
    }
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.slots.iter_mut().flatten()
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&usize, &mut T) -> bool) {
        for (key, slot) in self.slots.iter_mut().enumerate() {
            if let Some(value) = slot
                && !keep(&key, value)
            {
                *slot = None;
                self.len -= 1;
            }
        }
    }
}

impl<T> std::ops::Index<&usize> for Slots<T> {
    type Output = T;
    fn index(&self, key: &usize) -> &T {
        self.get(key).expect("no value at this index")
    }
}

impl<T> FromIterator<(usize, T)> for Slots<T> {
    fn from_iter<I: IntoIterator<Item = (usize, T)>>(entries: I) -> Self {
        let mut slots = Self::default();
        for (key, value) in entries {
            slots.insert(key, value);
        }
        slots
    }
}
