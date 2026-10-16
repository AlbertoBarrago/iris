//! Local contact lists, independent of any account's address book. Members
//! keep their addresses even when a provider stops returning a contact.

use rusqlite::{Connection, params};

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactList {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub email: String,
}

/// Creates an empty list. Names need not be unique; ids distinguish them.
pub fn create(conn: &Connection, name: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO contact_lists (name) VALUES (?1)",
        [name.trim()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Renames a list, returning whether it still exists.
pub fn rename(conn: &Connection, id: i64, name: &str) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE contact_lists SET name = ?2 WHERE id = ?1",
        params![id, name.trim()],
    )? > 0)
}

/// Lists by name, with ids breaking ties between equal names.
pub fn list(conn: &Connection) -> Result<Vec<ContactList>> {
    let mut stmt =
        conn.prepare("SELECT id, name FROM contact_lists ORDER BY name COLLATE NOCASE, id")?;
    let rows = stmt.query_map([], |row| {
        Ok(ContactList {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Removes the list and its membership, leaving all contacts alone.
pub fn remove(conn: &Connection, id: i64) -> Result<bool> {
    Ok(conn.execute("DELETE FROM contact_lists WHERE id = ?1", [id])? > 0)
}

/// Appends an address once, ignoring surrounding space and case. Repeated
/// additions leave the original name and position in place.
pub fn add_member(conn: &Connection, id: i64, member: &Member) -> Result<bool> {
    Ok(conn.execute(
        "INSERT INTO contact_list_members (list_id, email, name, rank) \
         SELECT ?1, ?2, ?3, coalesce(max(rank), -1) + 1 \
         FROM contact_list_members WHERE list_id = ?1 \
         ON CONFLICT (list_id, email) DO NOTHING",
        params![id, member.email.trim().to_lowercase(), member.name.trim()],
    )? > 0)
}

pub fn remove_member(conn: &Connection, id: i64, email: &str) -> Result<bool> {
    Ok(conn.execute(
        "DELETE FROM contact_list_members WHERE list_id = ?1 AND email = ?2",
        params![id, email.trim().to_lowercase()],
    )? > 0)
}

/// Members in insertion order, ready to expand into individual recipients.
pub fn members(conn: &Connection, id: i64) -> Result<Vec<Member>> {
    let mut stmt = conn.prepare(
        "SELECT name, email FROM contact_list_members WHERE list_id = ?1 ORDER BY rank, email",
    )?;
    let rows = stmt.query_map([id], |row| {
        Ok(Member {
            name: row.get(0)?,
            email: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{accounts, address_book, open_connection, open_in_memory};

    fn person(name: &str, email: &str) -> Member {
        Member {
            name: name.into(),
            email: email.into(),
        }
    }

    #[test]
    fn lists_and_their_members_survive_renaming_and_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mail.db");
        let id;
        {
            let conn = open_connection(&path).unwrap();
            id = create(&conn, " Family ").unwrap();
            assert!(members(&conn, id).unwrap().is_empty());
            add_member(&conn, id, &person(" Ana ", " Ana@Example.org ")).unwrap();
            add_member(&conn, id, &person("Rui", "rui@example.org")).unwrap();
            assert!(rename(&conn, id, " Friends ").unwrap());
        }
        let conn = open_connection(&path).unwrap();
        assert_eq!(
            list(&conn).unwrap(),
            vec![ContactList {
                id,
                name: "Friends".into()
            }]
        );
        assert_eq!(
            members(&conn, id).unwrap(),
            vec![
                person("Ana", "ana@example.org"),
                person("Rui", "rui@example.org")
            ]
        );
    }

    #[test]
    fn membership_is_unique_per_list_and_removal_preserves_the_other_members() {
        let conn = open_in_memory().unwrap();
        let family = create(&conn, "Family").unwrap();
        let work = create(&conn, "Team").unwrap();
        let ana = person("Ana", "ana@example.org");
        assert!(add_member(&conn, family, &ana).unwrap());
        assert!(!add_member(&conn, family, &person("Another name", " ANA@example.org ")).unwrap());
        add_member(&conn, family, &person("Rui", "rui@example.org")).unwrap();
        add_member(&conn, work, &ana).unwrap();
        assert_eq!(members(&conn, family).unwrap()[0], ana);
        assert!(remove_member(&conn, family, " ANA@example.org ").unwrap());
        assert!(!remove_member(&conn, family, "ana@example.org").unwrap());
        add_member(&conn, family, &ana).unwrap();
        assert_eq!(
            members(&conn, family).unwrap(),
            vec![person("Rui", "rui@example.org"), ana.clone()]
        );
        assert!(remove(&conn, family).unwrap());
        assert!(members(&conn, family).unwrap().is_empty());
        assert_eq!(members(&conn, work).unwrap(), vec![ana]);
    }

    #[test]
    fn removing_an_account_or_a_list_does_not_remove_the_other_address_book() {
        let conn = open_in_memory().unwrap();
        let account = accounts::insert_account(&conn, "me@example.org", 0).unwrap();
        address_book::save(
            &conn,
            &[address_book::Contact {
                account_id: account,
                resource: "people/ana".into(),
                name: Some("Ana".into()),
                emails: vec!["ana@example.org".into()],
                ..Default::default()
            }],
        )
        .unwrap();
        let id = create(&conn, "Family").unwrap();
        let ana = person("Ana", "ana@example.org");
        add_member(&conn, id, &ana).unwrap();
        remove(&conn, id).unwrap();
        assert_eq!(address_book::list(&conn).unwrap().len(), 1);
        let id = create(&conn, "Family").unwrap();
        add_member(&conn, id, &ana).unwrap();
        conn.execute("DELETE FROM accounts WHERE id = ?1", [account])
            .unwrap();
        assert_eq!(members(&conn, id).unwrap(), vec![ana]);
    }

    #[test]
    fn blank_names_and_addresses_are_rejected_without_losing_saved_values() {
        let conn = open_in_memory().unwrap();
        assert!(create(&conn, " \t\n").is_err());
        let id = create(&conn, "Family").unwrap();
        assert!(rename(&conn, id, " ").is_err());
        assert!(add_member(&conn, id, &person("Ana", " \n")).is_err());
        assert!(add_member(&conn, id + 1, &person("Ana", "ana@example.org")).is_err());
        assert_eq!(
            list(&conn).unwrap(),
            vec![ContactList {
                id,
                name: "Family".into()
            }]
        );
        assert!(members(&conn, id).unwrap().is_empty());
        assert!(!rename(&conn, id + 1, "Missing").unwrap());
        assert!(!remove(&conn, id + 1).unwrap());
    }
}
