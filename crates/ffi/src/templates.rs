//! Templates for the Swift front end: the saved bodies Preferences lists
//! and edits, and one filled in for the composer, its placeholders
//! standing for this message as `mailrs_appcore::templates` fills them in
//! the GTK composer.

use chrono::Local;
use mailrs_appcore::compose;
use mailrs_appcore::richtext::RichBody;
use mailrs_appcore::templates::{self, Filling};
use mailrs_domain::translate::{fill, gettext};
use mailrs_store::templates::Template;

use crate::CoreError;
use crate::compose::{RichBlock, rich_blocks, rich_body};
use crate::mail::Mail;

/// A saved template.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TemplateItem {
    pub id: i64,
    pub name: String,
    /// The subject it gives a message that has none; empty for none.
    pub subject: String,
    /// The body as Markdown, placeholders and all.
    pub markdown: String,
    /// The subject, or the body's first line, for the list.
    pub summary: String,
}

/// A template filled in for one message.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FilledTemplate {
    /// The subject to give the message, when it has none of its own.
    pub subject: Option<String>,
    /// The body to put in at the cursor.
    pub blocks: Vec<RichBlock>,
}

#[uniffi::export]
impl Mail {
    /// Every saved template, by name.
    pub fn templates(&self) -> Result<Vec<TemplateItem>, CoreError> {
        let db = self.db.clone();
        Ok(self
            .run(async move { db.read(mailrs_store::templates::list).await })?
            .into_iter()
            .map(|t| TemplateItem {
                summary: templates::summary(&t),
                id: t.id,
                name: t.name,
                subject: t.subject,
                markdown: t.markdown,
            })
            .collect())
    }

    /// Saves a template: a new one when `id` is 0, else the one `id`
    /// names. Answers its id.
    pub fn save_template(&self, id: i64, name: String, subject: String, markdown: String) -> Result<i64, CoreError> {
        let template = named(id, name, subject, markdown)?;
        let db = self.db.clone();
        self.run(async move {
            db.write(move |c| match template.id {
                0 => mailrs_store::templates::add(c, &template),
                id => mailrs_store::templates::update(c, &template).map(|()| id),
            })
            .await
        })
        .map_err(|err| CoreError::Store(fill(&gettext("Template not saved: {reason}"), &[("reason", &err.to_string())])))
    }

    /// Keeps what the composer holds as a new template called `name`. The
    /// body is saved as written, so its placeholders fill in again the
    /// next time it goes into a message.
    pub fn save_draft_as_template(&self, name: String, subject: String, blocks: Vec<RichBlock>) -> Result<i64, CoreError> {
        let markdown = rich_body(blocks).to_markdown();
        self.save_template(0, name, subject.trim().to_string(), markdown)
    }

    pub fn delete_template(&self, id: i64) -> Result<(), CoreError> {
        let db = self.db.clone();
        Ok(self.run(async move { db.write(move |c| mailrs_store::templates::remove(c, id)).await })?)
    }

    /// Template `id` filled in for a message to `to` (the recipient
    /// field's text) under `subject`.
    pub fn fill_template(&self, id: i64, to: String, subject: String) -> Result<FilledTemplate, CoreError> {
        let db = self.db.clone();
        let template = self
            .run(async move { db.read(mailrs_store::templates::list).await })?
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| CoreError::Store(gettext("The template is gone")))?;
        Ok(filled(&template, &to, &subject))
    }
}

/// A template to save, or the reason it cannot be.
fn named(id: i64, name: String, subject: String, markdown: String) -> Result<Template, CoreError> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(CoreError::Store(gettext("Give the template a name")));
    }
    Ok(Template {
        id,
        name,
        subject: subject.trim().to_string(),
        markdown,
    })
}

fn filled(template: &Template, to: &str, subject: &str) -> FilledTemplate {
    let mut filling = Filling {
        recipient: compose::parse_recipients(to).into_iter().next(),
        subject: subject.trim().to_string(),
        date: templates::today(Local::now()),
    };
    let new_subject = templates::subject_for(template, &filling);
    if let Some(given) = &new_subject {
        filling.subject = given.clone();
    }
    let body = templates::fill(&RichBody::from_markdown(&template.markdown), &filling);
    FilledTemplate {
        subject: new_subject,
        blocks: rich_blocks(body),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thanks() -> Template {
        Template {
            id: 1,
            name: "Thanks".into(),
            subject: "Thanks, {{first_name}}".into(),
            markdown: "Hi {{first_name}},\n\n**Thank you** for {{subject}}.".into(),
        }
    }

    #[test]
    fn a_template_greets_the_first_recipient() {
        let done = filled(&thanks(), "Ann Lee <ann@example.com>, bo@example.com", "");
        assert_eq!(done.subject.as_deref(), Some("Thanks, Ann"));
        let words: Vec<String> =
            done.blocks.iter().map(|b| b.spans.iter().map(|s| s.text.as_str()).collect()).collect();
        assert_eq!(words, ["Hi Ann,", "", "Thank you for Thanks, Ann."]);
        assert!(done.blocks[2].spans[0].bold);
    }

    #[test]
    fn the_writer_keeps_their_own_subject() {
        let done = filled(&thanks(), "", "Lunch");
        assert_eq!(done.subject, None);
        assert_eq!(done.blocks[2].spans.last().map(|s| s.text.as_str()), Some(" for Lunch."));
    }

    #[test]
    fn a_template_needs_a_name() {
        assert!(named(0, "  ".into(), String::new(), String::new()).is_err());
        assert_eq!(named(3, " Thanks ".into(), " Hi ".into(), "x".into()).map(|t| (t.id, t.name, t.subject)).ok(),
            Some((3, "Thanks".to_string(), "Hi".to_string())));
    }
}
