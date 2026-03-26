//! A helper module to mock the interface of `sysinfo` for testing.
//! Mainly there to allow constructing `Components` by hand.

pub struct Components(Vec<Component>);

impl Components {
    pub fn new_with_refreshed_list() -> Self {
        Self(vec![])
    }

    pub fn list(&self) -> &[Component] {
        &self.0
    }

    pub fn refresh(&mut self, _remove_not_listed_components: bool) {}
}

impl From<Vec<Component>> for Components {
    fn from(v: Vec<Component>) -> Self {
        Self(v)
    }
}

impl std::ops::Deref for Components {
    type Target = [Component];

    fn deref(&self) -> &Self::Target {
        self.list()
    }
}

impl<'a> IntoIterator for &'a Components {
    type Item = &'a Component;
    type IntoIter = std::slice::Iter<'a, Component>;

    fn into_iter(self) -> Self::IntoIter {
        self.list().iter()
    }
}

pub struct Component {
    pub label: &'static str,
    // the original is `Option<f64>`, but this is simplified to help with mocking
    pub temperature: f32,
}

impl Component {
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn temperature(&self) -> Option<f32> {
        Some(self.temperature)
    }
}
