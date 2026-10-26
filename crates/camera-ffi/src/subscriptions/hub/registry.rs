use super::*;

const CAPACITY: usize = 64;

pub(in crate::subscriptions) struct Subscribers {
    rows: [Weak<Subscription>; CAPACITY],
    len: usize,
}
impl Default for Subscribers {
    fn default() -> Self {
        Self {
            rows: std::array::from_fn(|_| Weak::new()),
            len: 0,
        }
    }
}
impl Subscribers {
    pub(in crate::subscriptions) fn len(&self) -> usize {
        self.len
    }
    pub(in crate::subscriptions) fn is_full(&self) -> bool {
        self.len == CAPACITY
    }
    pub(in crate::subscriptions) fn iter(&self) -> std::slice::Iter<'_, Weak<Subscription>> {
        self.rows[..self.len].iter()
    }
    #[cfg(test)]
    pub(in crate::subscriptions) fn push(&mut self, sub: Weak<Subscription>) -> Result<()> {
        if self.is_full() {
            return Err(AM_ERR_CAMERA_SUBSCRIBER_LIMIT_REACHED);
        }
        self.push_reserved(sub);
        Ok(())
    }
    pub(in crate::subscriptions) fn push_reserved(&mut self, sub: Weak<Subscription>) {
        assert!(
            !self.is_full(),
            "subscriber slot must be checked while locked"
        );
        self.rows[self.len] = sub;
        self.len += 1;
    }
    pub(in crate::subscriptions) fn prune(&mut self) {
        let mut retained = 0;
        for index in 0..self.len {
            if self.rows[index]
                .upgrade()
                .is_some_and(|sub| sub.occupies_slot())
            {
                self.rows.swap(retained, index);
                retained += 1;
            }
        }
        for row in &mut self.rows[retained..self.len] {
            *row = Weak::new();
        }
        self.len = retained;
    }
    pub(in crate::subscriptions) fn snapshot(&self) -> Snapshot {
        let mut snapshot = Snapshot::default();
        for sub in self.iter().filter_map(Weak::upgrade) {
            if !sub.stopped() {
                snapshot.rows[snapshot.len] = Some(sub);
                snapshot.len += 1;
            }
        }
        snapshot
    }
}

// 固定栈快照保留Arc，使锁外投递期间对象不会释放。
pub(in crate::subscriptions) struct Snapshot {
    rows: [Option<Arc<Subscription>>; CAPACITY],
    len: usize,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            rows: std::array::from_fn(|_| None),
            len: 0,
        }
    }
}
impl Snapshot {
    pub(in crate::subscriptions) fn iter(&self) -> impl Iterator<Item = &Arc<Subscription>> {
        self.rows[..self.len].iter().filter_map(Option::as_ref)
    }
    #[cfg(test)]
    pub(in crate::subscriptions) fn len(&self) -> usize {
        self.len
    }
    #[cfg(test)]
    pub(in crate::subscriptions) fn is_empty(&self) -> bool {
        self.len == 0
    }
}
#[cfg(test)]
impl FromIterator<Arc<Subscription>> for Snapshot {
    fn from_iter<T: IntoIterator<Item = Arc<Subscription>>>(iter: T) -> Self {
        let mut snapshot = Self::default();
        for sub in iter {
            assert!(snapshot.len < CAPACITY);
            snapshot.rows[snapshot.len] = Some(sub);
            snapshot.len += 1;
        }
        snapshot
    }
}

#[cfg(test)]
mod tests;
