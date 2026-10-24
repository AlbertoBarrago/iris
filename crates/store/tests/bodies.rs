mod common;

use common::{db, meta, store};
use mailrs_domain::{Address, Attachment, MessageBody, Protection, Provenance, Reaction};
use mailrs_store::messages::Change;
use mailrs_store::{bodies, messages};

fn body(text: &str) -> MessageBody {
    MessageBody {
        html: Some(format!("<p>{text}</p>")),
        text: Some(text.into()),
        attachments: vec![Attachment {
            part_id: "2".into(),
            filename: "a.pdf".into(),
            mime_type: "application/pdf".into(),
            size: 10,
            attachment_id: Some("att".into()),
            content_id: None,
        }],
        list_unsubscribe: Some("<https://news.example/u>".into()),
        one_click_unsubscribe: true,
        calendar: Some("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n".into()),
        protection: Some(Protection::Signed),
        provenance: Provenance {
            mailed_by: Some("bounce.example.net".into()),
            signed_by: Some("example.net".into()),
            encrypted: Some(true),
        },
        reply_to: vec![Address {
            name: Some("The list".into()),
            email: "list@news.example".into(),
        }],
        reaction: None,
    }
}

#[test]
fn bodies_round_trip_with_attachments() {
    let (conn, id) = db();
    store(&conn, &[meta(id, "a", "t1", 100, &["INBOX"])]);
    bodies::put_body(&conn, id, "a", &body("hi"), 1).unwrap();
    assert_eq!(
        bodies::get_body(&conn, id, "a", 2).unwrap(),
        Some(body("hi"))
    );
    assert_eq!(bodies::get_body(&conn, id, "missing", 2).unwrap(), None);
}

#[test]
fn every_wrapper_a_message_can_arrive_in_survives_the_store() {
    let (conn, id) = db();
    store(&conn, &[meta(id, "a", "t1", 100, &["INBOX"])]);
    for (at, protection) in Protection::ALL.into_iter().enumerate() {
        let kept = MessageBody {
            protection: Some(protection),
            ..body("hi")
        };
        bodies::put_body(&conn, id, "a", &kept, at as i64).unwrap();
        assert_eq!(
            bodies::get_body(&conn, id, "a", at as i64).unwrap(),
            Some(kept),
            "{protection:?}"
        );
    }
}

#[test]
fn eviction_keeps_the_most_recently_read_bodies() {
    let (conn, id) = db();
    store(
        &conn,
        &[
            meta(id, "a", "t1", 100, &[]),
            meta(id, "b", "t2", 100, &[]),
            meta(id, "c", "t3", 100, &[]),
        ],
    );
    // Each body is 15 bytes: "<p>xxxx</p>" plus "xxxx".
    bodies::put_body(&conn, id, "a", &body("aaaa"), 1).unwrap();
    bodies::put_body(&conn, id, "b", &body("bbbb"), 2).unwrap();
    bodies::put_body(&conn, id, "c", &body("cccc"), 3).unwrap();
    bodies::get_body(&conn, id, "a", 4).unwrap();
    assert_eq!(bodies::evict_bodies(&conn, 30).unwrap(), 1);
    assert!(bodies::get_body(&conn, id, "b", 5).unwrap().is_none());
    assert!(bodies::get_body(&conn, id, "a", 5).unwrap().is_some());
    assert!(bodies::get_body(&conn, id, "c", 5).unwrap().is_some());
}

#[test]
fn deleting_a_message_deletes_its_body() {
    let (conn, id) = db();
    store(&conn, &[meta(id, "a", "t1", 100, &["INBOX"])]);
    bodies::put_body(&conn, id, "a", &body("hi"), 1).unwrap();
    let delete = Change::Delete {
        message_id: "a".into(),
    };
    messages::apply(&conn, id, &[delete]).unwrap();
    assert!(bodies::get_body(&conn, id, "a", 2).unwrap().is_none());
    let attachments: i64 = conn
        .query_row("SELECT COUNT(*) FROM attachments", [], |r| r.get(0))
        .unwrap();
    assert_eq!(attachments, 0);
}

#[test]
fn putting_a_body_again_replaces_it() {
    let (conn, id) = db();
    store(&conn, &[meta(id, "a", "t1", 100, &["INBOX"])]);
    bodies::put_body(&conn, id, "a", &body("one"), 1).unwrap();
    let plain = MessageBody {
        html: None,
        text: Some("two".into()),
        ..Default::default()
    };
    bodies::put_body(&conn, id, "a", &plain, 2).unwrap();
    assert_eq!(bodies::get_body(&conn, id, "a", 3).unwrap(), Some(plain));
}

#[test]
fn a_cache_under_the_cap_keeps_every_body() {
    let (conn, id) = db();
    store(
        &conn,
        &[meta(id, "a", "t1", 100, &[]), meta(id, "b", "t2", 100, &[])],
    );
    bodies::put_body(&conn, id, "a", &body("aaaa"), 1).unwrap();
    bodies::put_body(&conn, id, "b", &body("bbbb"), 2).unwrap();
    assert_eq!(bodies::evict_bodies(&conn, 30).unwrap(), 0);
    assert_eq!(bodies::evict_bodies(&conn, 29).unwrap(), 1);
    assert!(bodies::peek_body(&conn, id, "a").unwrap().is_none());
    let attachments: i64 = conn
        .query_row("SELECT COUNT(*) FROM attachments", [], |r| r.get(0))
        .unwrap();
    assert_eq!(attachments, 1, "the evicted body took its attachments");
}

#[test]
fn recorded_reads_only_move_access_times_forward() {
    let (conn, id) = db();
    store(
        &conn,
        &[meta(id, "a", "t1", 100, &[]), meta(id, "b", "t2", 100, &[])],
    );
    bodies::put_body(&conn, id, "a", &body("aaaa"), 1).unwrap();
    bodies::put_body(&conn, id, "b", &body("bbbb"), 2).unwrap();
    bodies::touch_bodies(
        &conn,
        id,
        &[("a".into(), 9), ("b".into(), 1), ("gone".into(), 9)],
    )
    .unwrap();
    // "a" was read last, so the older "b" goes first.
    assert_eq!(bodies::evict_bodies(&conn, 15).unwrap(), 1);
    assert!(bodies::peek_body(&conn, id, "a").unwrap().is_some());
    assert!(bodies::peek_body(&conn, id, "b").unwrap().is_none());
}

/// A reaction to the first message: it outlives its cached body, does not
/// count as a message of the thread, and gives the row its emoji.
#[test]
fn a_reaction_folds_into_the_message_it_answers() {
    use mailrs_store::{messages::ThreadReaction, threads};

    let (conn, id) = db();
    store(&conn, &[meta(id, "a", "t1", 100, &["INBOX"]), meta(id, "r", "t1", 200, &["INBOX"])]);
    assert_eq!(threads::get_thread(&conn, id, "t1").unwrap().unwrap().message_count, 2);
    let reaction = MessageBody {
        text: Some("Mario reacted via Gmail".into()),
        reaction: Some(Reaction { emoji: "👍".into(), to: "<a@example.com>".into() }),
        ..MessageBody::default()
    };
    bodies::put_body(&conn, id, "r", &reaction, 1).unwrap();
    let row = threads::get_thread(&conn, id, "t1").unwrap().unwrap();
    assert_eq!((row.message_count, row.snippet.as_str()), (1, "👍"));
    assert_eq!(
        messages::thread_reactions(&conn, id, "t1").unwrap(),
        vec![ThreadReaction { message_id: "r".into(), emoji: "👍".into(), to: "<a@example.com>".into() }]
    );
    assert_eq!(bodies::get_body(&conn, id, "r", 2).unwrap().unwrap().reaction, reaction.reaction);
    // Evicting every body leaves the reaction on the message.
    bodies::evict_bodies(&conn, 0).unwrap();
    assert_eq!(messages::thread_reactions(&conn, id, "t1").unwrap().len(), 1);
}

/// A reaction to a message the thread does not hold stays a message of
/// its own, as Gmail shows it.
#[test]
fn a_reaction_to_a_message_elsewhere_still_counts() {
    use mailrs_store::threads;

    let (conn, id) = db();
    store(&conn, &[meta(id, "a", "t1", 100, &["INBOX"]), meta(id, "r", "t1", 200, &["INBOX"])]);
    let reaction = MessageBody {
        reaction: Some(Reaction { emoji: "🎉".into(), to: "<gone@example.com>".into() }),
        ..MessageBody::default()
    };
    bodies::put_body(&conn, id, "r", &reaction, 1).unwrap();
    assert_eq!(threads::get_thread(&conn, id, "t1").unwrap().unwrap().message_count, 2);
}
