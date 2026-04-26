#![allow(dead_code)]

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum FeatureUse<T> {
    #[default]
    Inherit,
    Add(T),
    Replace(T),
    Off,
}

impl<T> FeatureUse<T> {
    #[inline]
    pub fn is_inherit(&self) -> bool {
        matches!(self, Self::Inherit)
    }

    #[inline]
    pub fn is_off(&self) -> bool {
        matches!(self, Self::Off)
    }

    #[inline]
    pub fn as_ref(&self) -> FeatureUse<&T> {
        match self {
            Self::Inherit => FeatureUse::Inherit,
            Self::Add(value) => FeatureUse::Add(value),
            Self::Replace(value) => FeatureUse::Replace(value),
            Self::Off => FeatureUse::Off,
        }
    }
}

impl<T: Clone> FeatureUse<T> {
    #[inline]
    pub fn merge_inherited(
        parent: Option<T>,
        patch: Self,
        add: impl FnOnce(T, T) -> T,
    ) -> Option<T> {
        match (parent, patch) {
            (parent, Self::Inherit) => parent,
            (None, Self::Add(value)) | (_, Self::Replace(value)) => Some(value),
            (Some(parent), Self::Add(value)) => Some(add(parent, value)),
            (_, Self::Off) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FeatureUse;

    #[test]
    fn feature_use_merge_semantics() {
        assert_eq!(
            FeatureUse::merge_inherited(Some(vec![1]), FeatureUse::Inherit, |mut a, b| {
                a.extend(b);
                a
            }),
            Some(vec![1])
        );
        assert_eq!(
            FeatureUse::merge_inherited(Some(vec![1]), FeatureUse::Add(vec![2]), |mut a, b| {
                a.extend(b);
                a
            }),
            Some(vec![1, 2])
        );
        assert_eq!(
            FeatureUse::merge_inherited(Some(vec![1]), FeatureUse::Replace(vec![3]), |mut a, b| {
                a.extend(b);
                a
            }),
            Some(vec![3])
        );
        assert_eq!(
            FeatureUse::merge_inherited(Some(vec![1]), FeatureUse::<Vec<i32>>::Off, |mut a, b| {
                a.extend(b);
                a
            }),
            None
        );
    }
}
