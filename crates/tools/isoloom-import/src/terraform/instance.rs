//! A resource instance after evaluation: its type, its name (with its key when expanded), and
//! its attributes as JSON, unresolved expressions kept as their text.

use serde_json::Value as Json;

/// One resource instance.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Instance {
    /// The resource type (`aws_instance`).
    pub(super) kind: String,
    /// The resource's name (`vm`).
    pub(super) name: String,
    /// Its key, when `for_each` or `count` expanded the resource (`dc01`, `0`).
    pub(super) key: Option<String>,
    /// Its attributes and nested blocks (blocks of one name as a list).
    pub(super) attrs: Json,
}

impl Instance {
    /// How other resources refer to it: `aws_subnet.lab`, `aws_instance.vm["dc01"]`, `x.y[0]`.
    pub(super) fn address(&self) -> String {
        match &self.key {
            Some(k) if k.parse::<usize>().is_ok() => format!("{}.{}[{k}]", self.kind, self.name),
            Some(k) => format!("{}.{}[\"{k}\"]", self.kind, self.name),
            None => format!("{}.{}", self.kind, self.name),
        }
    }

    /// The attribute at `path`, through the first of each nested block; none when unset.
    pub(super) fn get(&self, path: &[&str]) -> Option<&Json> {
        let mut v = &self.attrs;
        for p in path {
            v = match v {
                Json::Array(a) => a.first()?.get(*p)?,
                other => other.get(*p)?,
            };
        }
        Some(v).filter(|found| !found.is_null())
    }

    /// The attribute at `path` as text: a string, a number, or a list's first string.
    pub(super) fn str(&self, path: &[&str]) -> Option<String> {
        match self.get(path)? {
            Json::String(s) => Some(s.clone()),
            Json::Number(n) => Some(n.to_string()),
            Json::Array(a) => a.first().and_then(Json::as_str).map(String::from),
            _ => None,
        }
    }

    /// What names it in the draft: its key when it was expanded, else `named` (an attribute
    /// that names it, when it has one), else the resource's name.
    pub(super) fn name_or(&self, named: Option<String>) -> String {
        self.key.clone().or(named).unwrap_or_else(|| self.name.clone())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn instance(key: Option<&str>, attrs: Json) -> Instance {
        Instance {
            kind: "aws_instance".to_owned(),
            name: "vm".to_owned(),
            key: key.map(str::to_owned),
            attrs,
        }
    }

    #[test]
    fn an_instance_is_addressed_as_terraform_refers_to_it() {
        assert_eq!(instance(None, json!({})).address(), "aws_instance.vm");
        assert_eq!(instance(Some("0"), json!({})).address(), "aws_instance.vm[0]");
        assert_eq!(instance(Some("dc01"), json!({})).address(), "aws_instance.vm[\"dc01\"]");
    }

    #[test]
    fn an_attribute_is_read_through_the_first_nested_block() {
        let i = instance(
            None,
            json!({ "network_interface": [{ "network_ip": "10.0.0.5" }], "private_ips": ["10.0.0.6"], "gone": null }),
        );
        assert_eq!(i.str(&["network_interface", "network_ip"]).as_deref(), Some("10.0.0.5"));
        assert_eq!(i.str(&["private_ips"]).as_deref(), Some("10.0.0.6"));
        assert_eq!(i.get(&["gone"]), None);
    }
}
