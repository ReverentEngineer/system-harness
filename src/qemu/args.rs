use core::fmt::Debug;
use core::fmt::Display;

pub trait Backend {
    /// Name of the backend
    fn name(&self) -> &str;

    /// Properties of backend
    fn properties<'a>(&'a self) -> PropertyList<'a>;
}

pub trait PropertyValue {
    fn value(&self) -> Option<String>;
}

impl PropertyValue for &str {
    fn value(&self) -> Option<String> {
        Some(self.to_string())
    }
}

impl PropertyValue for usize {
    fn value(&self) -> Option<String> {
        Some(format!("{}", self))
    }
}

impl PropertyValue for String {
    fn value(&self) -> Option<String> {
        Some(self.clone())
    }
}

impl<T> PropertyValue for Option<T>
where
    T: PropertyValue,
{
    fn value(&self) -> Option<String> {
        match self {
            Some(prop) => prop.value(),
            None => None,
        }
    }
}

pub struct Property<'prop> {
    key: &'prop str,
    value: &'prop dyn PropertyValue,
}

impl Debug for Property<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(value) = self.value.value() {
            write!(f, "{}={}", self.key, value)?;
        }
        Ok(())
    }
}

impl Display for Property<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = self.value.value().unwrap_or(String::new());
        write!(f, "{}={}", self.key, value)
    }
}

impl<'prop> Property<'prop> {
    pub fn valued(&self) -> Option<ValuedProperty<'prop>> {
        self.value.value().map(|value| ValuedProperty {
            key: self.key,
            value,
        })
    }
}

pub struct ValuedProperty<'prop> {
    key: &'prop str,
    value: String,
}

impl Display for ValuedProperty<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}={}", self.key, self.value)
    }
}

#[derive(Default, Debug)]
pub struct PropertyList<'list>(Vec<Property<'list>>);

impl<'list> PropertyList<'list> {
    #[allow(dead_code)]
    pub(crate) fn insert(&mut self, key: &'list str, value: &'list dyn PropertyValue) {
        self.0.push(Property { key, value })
    }
}

impl Display for PropertyList<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut props = self.0.iter().filter_map(Property::valued);
        if let Some(property) = props.next() {
            write!(f, "{property}")?;
        }
        for property in props {
            write!(f, ",{property}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use cmdstruct::Arg;
    use serde::Deserialize;
    use std::collections::BTreeMap;
    use system_harness_macros::PropertyList;

    #[test]
    fn property_list_single() {
        let mut props = PropertyList::default();
        props.insert("a", &"123");
        assert_eq!("a=123".to_string(), format!("{props}"));
    }

    #[test]
    fn property_list() {
        let mut props = PropertyList::default();
        props.insert("a", &"321");
        props.insert("b", &"4");
        assert_eq!("a=321,b=4".to_string(), format!("{props}"));
    }

    #[test]
    fn derive() {
        #[derive(PropertyList, Deserialize)]
        struct Test {
            x: String,
            y: usize,
            z: String,
            #[serde(flatten)]
            a: BTreeMap<String, String>,
        }
        let mut a = BTreeMap::new();
        a.insert("b".to_string(), "1".to_string());
        a.insert("c".to_string(), "2".to_string());
        let test = Test {
            x: "abc".to_string(),
            y: 3,
            z: "123".to_string(),
            a,
        };

        let mut command = std::process::Command::new("test");
        test.append_arg(&mut command);
        assert_eq!(
            vec!["x=abc,y=3,z=123,b=1,c=2"],
            command.get_args().collect::<Vec<_>>()
        );
    }
}
