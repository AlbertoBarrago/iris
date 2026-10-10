//! Suggestions while typing a search, as Apple Mail offers them.

pub mod typing;

use mailrs_store::contacts::Suggestion as Person;

use crate::contacts::suggest;
use mailrs_domain::translate::{fill, gettext};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// What the list shows.
    pub label: String,
    /// The whole search to run when picked.
    pub query: String,
}

/// Suggestions for the word being typed at the end of `text`: the subject,
/// people it could be from or to, and labels it could name.
pub fn suggestions(text: &str, contacts: &[Person], labels: &[String]) -> Vec<Suggestion> {
    if text.ends_with(char::is_whitespace) {
        return Vec::new();
    }
    let start = text.rfind(char::is_whitespace).map_or(0, |i| i + 1);
    let (head, word) = (&text[..start], &text[start..]);
    if word.len() < 2 || word.contains(':') {
        return Vec::new();
    }
    let with = |term: String| format!("{head}{term}");
    // The query beside each label is Gmail's search language and stays
    // as Gmail spells it.
    let mut out = vec![Suggestion {
        label: fill(&gettext("Subject contains “{words}”"), &[("words", word)]),
        query: with(format!("subject:{word}")),
    }];
    let people = suggest(contacts, word, &[], 4, None);
    // A sender who writes from several addresses, such as a shop's info@
    // and no-reply@, would show one name several times; the address
    // tells those rows apart.
    let shown = |person: &Person| match person.name.as_deref() {
        Some(name) if people.iter().filter(|p| p.name.as_deref() == Some(name)).count() > 1 => {
            format!("{name} <{}>", person.email)
        }
        Some(name) => name.to_string(),
        None => person.email.clone(),
    };
    for person in &people {
        out.push(Suggestion {
            label: fill(&gettext("From {person}"), &[("person", &shown(person))]),
            query: with(format!("from:{}", person.email)),
        });
    }
    if let Some(first) = people.first() {
        out.push(Suggestion {
            label: fill(&gettext("To {person}"), &[("person", &shown(first))]),
            query: with(format!("to:{}", first.email)),
        });
    }
    let lower = word.to_lowercase();
    for label in labels
        .iter()
        .filter(|l| l.to_lowercase().contains(&lower))
        .take(3)
    {
        let slug: String = label
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_whitespace() || c == '/' {
                    '-'
                } else {
                    c
                }
            })
            .collect();
        out.push(Suggestion {
            label: fill(
                &gettext("In {label}"),
                &[("label", &label.replace('/', " › "))],
            ),
            query: with(format!("label:{slug}")),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_word_becomes_subject_people_and_labels() {
        let contacts = [Person {
            name: Some("Ann Lee".into()),
            email: "ann@example.com".into(),
            organization: None,
            photo_file: None,
            accounts: vec![1],
            score: 3,
            last_seen: 0,
        }];
        let labels = ["Work/Annual".to_string(), "Travel".to_string()];
        let found = suggestions("invoice an", &contacts, &labels);
        let queries: Vec<&str> = found.iter().map(|s| s.query.as_str()).collect();
        assert_eq!(
            queries,
            [
                "invoice subject:an",
                "invoice from:ann@example.com",
                "invoice to:ann@example.com",
                "invoice label:work-annual",
            ]
        );
        assert_eq!(found[1].label, "From Ann Lee");
        assert!(suggestions("invoice ", &contacts, &labels).is_empty());
        assert!(suggestions("from:an", &contacts, &labels).is_empty());
        assert!(suggestions("a", &contacts, &labels).is_empty());
    }

    #[test]
    fn a_name_with_several_addresses_shows_each_address() {
        let person = |email: &str| Person {
            name: Some("Deliveroo".into()),
            email: email.into(),
            organization: None,
            photo_file: None,
            accounts: vec![1],
            score: 3,
            last_seen: 0,
        };
        let contacts = [person("info@deliveroo.it"), person("no-reply@deliveroo.it")];
        let labels: Vec<String> = suggestions("deli", &contacts, &[])
            .into_iter()
            .map(|s| s.label)
            .collect();
        assert!(labels.contains(&"From Deliveroo <info@deliveroo.it>".to_string()));
        assert!(labels.contains(&"From Deliveroo <no-reply@deliveroo.it>".to_string()));
    }
}
