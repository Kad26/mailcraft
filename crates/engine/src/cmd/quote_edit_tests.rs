// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::quote_tests::{run, session};
use mailcraft_model::{Address, Body, Message};
use serde_json::json;

fn original(s: &mut crate::Session, html: &str) -> u64 {
    let account = s.mailbox.accounts[0].id;
    let folder = s.mailbox.inbox().unwrap();
    s.mailbox
        .add_message(Message {
            account,
            folder,
            from: Address::new("Ada", "ada@example.org"),
            subject: "Editable original".into(),
            body: Body { text: "Keep this. Remove that.".into(), html: Some(html.into()) },
            ..Default::default()
        })
        .unwrap()
}

#[test]
fn forward_original_can_be_selected_copied_cut_pasted_and_deleted() {
    let mut s = session();
    let id = original(&mut s, "<p><b>Keep this.</b> Remove that.</p><table><tr><td>Part</td><td>12,30</td></tr></table>");
    run(&mut s, "mail.forward", json!({"id": id}));
    let quote = s.compose.as_ref().unwrap().quote.clone();
    run(&mut s, "compose.quoteAsText", json!({}));
    assert!(s.compose.as_ref().unwrap().quote.is_none());
    let body = s.compose.as_ref().unwrap().body.text();
    assert!(body.paragraphs().iter().flat_map(|p| &p.runs).any(|r| r.table.is_some()));
    let place = body.find("Remove that.", true, 0, body.len())[0];
    run(&mut s, "compose.select", json!({"from":place.0, "to":place.1}));
    let copied = run(&mut s, "edit.copy", json!({}));
    assert_eq!(copied["text"], "Remove that.");
    run(&mut s, "edit.cut", json!({}));
    assert!(!s.compose.as_ref().unwrap().body.plain().contains("Remove that."));
    run(&mut s, "edit.paste", json!({"html": copied["html"], "text": copied["text"]}));
    assert!(s.compose.as_ref().unwrap().body.plain().contains("Remove that."));
    run(&mut s, "compose.undo", json!({}));
    run(&mut s, "compose.undo", json!({}));
    run(&mut s, "compose.undo", json!({}));
    assert_eq!(s.compose.as_ref().unwrap().quote, quote);
    run(&mut s, "compose.redo", json!({}));
    let body = s.compose.as_ref().unwrap().body.text();
    let place = body.find("Remove that.", true, 0, body.len())[0];
    run(&mut s, "compose.delete", json!({"from":place.0, "to":place.1}));
    let saved = run(&mut s, "compose.saveDraft", json!({}))["draft"].as_u64().unwrap();
    run(&mut s, "compose.close", json!({}));
    run(&mut s, "mail.editDraft", json!({"id":saved}));
    assert!(s.compose.as_ref().unwrap().quote.is_none());
    let text = s.compose.as_ref().unwrap().body.plain();
    assert!(text.contains("Keep this.") && !text.contains("Remove that."));
}

#[test]
fn reply_keeps_original_until_conversion_and_missing_images_refuse_atomically() {
    let mut s = session();
    let id = original(&mut s, "<p>Keep this.</p><img src=\"https://example.org/logo.png\">");
    run(&mut s, "mail.reply", json!({"id": id}));
    let before = s.compose.clone().unwrap();
    assert!(s.run("compose.quoteAsText", &json!({})).is_err());
    assert_eq!(s.compose.as_ref().unwrap().body, before.body);
    assert_eq!(s.compose.as_ref().unwrap().quote, before.quote);
}

#[test]
fn reply_original_only_becomes_editable_when_asked_and_undo_restores_it() {
    let mut s = session();
    let html = "<p>Keep <b>this.</b> Remove that.</p>";
    let id = original(&mut s, html);
    run(&mut s, "mail.reply", json!({"id":id}));
    let original_quote = s.compose.as_ref().unwrap().quote.clone();
    assert!(original_quote.is_some());
    assert!(!s.compose.as_ref().unwrap().body.plain().contains("Remove that."));
    run(&mut s, "compose.quoteAsText", json!({}));
    assert!(s.compose.as_ref().unwrap().body.plain().contains("Remove that."));
    run(&mut s, "compose.undo", json!({}));
    assert_eq!(s.compose.as_ref().unwrap().quote, original_quote);
    assert_eq!(s.mailbox.message(id).unwrap().body.html.as_deref(), Some(html));
}

#[test]
fn a_plain_text_forward_keeps_its_existing_selective_editing_flow() {
    let mut s = session();
    let id = original(&mut s, "");
    s.mailbox.message_mut(id).unwrap().body.html = None;
    run(&mut s, "mail.forward", json!({"id":id}));
    assert!(s.compose.as_ref().unwrap().quote.is_none());
    let text = s.compose.as_ref().unwrap().body.text();
    let place = text.find("Remove that.", true, 0, text.len())[0];
    run(&mut s, "compose.select", json!({"from":place.0,"to":place.1}));
    assert_eq!(run(&mut s, "edit.cut", json!({}))["text"], "Remove that.");
    assert!(s.compose.as_ref().unwrap().body.plain().contains("Keep this."));
    assert!(!s.compose.as_ref().unwrap().body.plain().contains("Remove that."));
    assert!(s.mailbox.message(id).unwrap().body.text.contains("Remove that."));
}

#[test]
fn an_oversized_original_is_not_truncated_by_conversion() {
    let mut s = session();
    let id = original(&mut s, &format!("<p>{}</p>", "x".repeat(300_000)));
    run(&mut s, "mail.forward", json!({"id": id}));
    let quote = s.compose.as_ref().unwrap().quote.clone();
    assert!(s.run("compose.quoteAsText", &json!({})).is_err());
    assert_eq!(s.compose.as_ref().unwrap().quote, quote);
}

#[test]
fn the_whole_original_can_be_removed_even_when_it_cannot_be_converted() {
    for html in ["<p>Remote original</p><img src=\"https://example.org/logo.png\">".to_string(), format!("<p>{}</p>", "x".repeat(300_000))] {
        let mut s = session();
        let id = original(&mut s, &html);
        run(&mut s, "mail.forward", json!({"id":id}));
        run(&mut s, "compose.set", json!({"body":"Keep my introduction"}));
        let before = s.compose.clone().unwrap();
        assert!(s.run("compose.quoteAsText", &json!({})).is_err());
        run(&mut s, "compose.removeQuote", json!({}));
        assert!(s.compose.as_ref().unwrap().quote.is_none());
        assert_eq!(s.compose.as_ref().unwrap().body.plain(), "Keep my introduction");
        run(&mut s, "compose.undo", json!({}));
        assert_eq!(s.compose.as_ref().unwrap().quote, before.quote);
        run(&mut s, "compose.redo", json!({}));
        let saved = run(&mut s, "compose.saveDraft", json!({}))["draft"].as_u64().unwrap();
        assert!(!s.mailbox.message(saved).unwrap().body.text.contains("Remote original"));
    }
}

#[test]
fn unavailable_pictures_only_leave_the_editable_copy_after_explicit_opt_in() {
    let mut s = session();
    let id = original(&mut s, "<p>Keep this. Remove that.</p><img src=\"https://example.org/logo.png\">");
    run(&mut s, "mail.forward", json!({"id":id}));
    let quote = s.compose.as_ref().unwrap().quote.clone();
    assert!(s.run("compose.quoteAsText", &json!({})).is_err());
    let result = run(&mut s, "compose.quoteAsText", json!({"omitUnavailablePictures":true}));
    assert_eq!(result["omittedPictures"], 1);
    let end = s.compose.as_ref().unwrap().body.text().len();
    run(&mut s, "compose.select", json!({"from":0, "to":end}));
    run(&mut s, "compose.delete", json!({}));
    assert!(s.compose.as_ref().unwrap().body.text().is_empty());
    run(&mut s, "compose.undo", json!({}));
    run(&mut s, "compose.undo", json!({}));
    assert_eq!(s.compose.as_ref().unwrap().quote, quote);
}

#[test]
fn converted_inline_pictures_are_kept_through_undo_and_save() {
    let mut s = session();
    let id = original(&mut s, "<p>Keep this.</p><img src=\"cid:logo@example.org\"><img src=\"cid:logo@example.org\">");
    let picture = mailcraft_model::Attachment {
        content_id: "logo@example.org".into(),
        inline: true,
        ..mailcraft_model::Attachment::new("logo.png", "image/png", b"\x89PNG\r\n\x1a\n".to_vec())
    };
    s.mailbox.message_mut(id).unwrap().attachments.push(picture);
    run(&mut s, "mail.forward", json!({"id":id}));
    run(&mut s, "compose.quoteAsText", json!({}));
    assert_eq!(s.compose.as_ref().unwrap().pictures.len(), 1);
    assert!(s.compose.as_ref().unwrap().attachments.is_empty());
    assert_eq!(s.compose.as_ref().unwrap().body.text().pictures().len(), 2);
    s.compose.as_mut().unwrap().attachments.push(mailcraft_model::Attachment::new("added.pdf", "application/pdf", vec![1, 2, 3]));
    run(&mut s, "compose.undo", json!({}));
    assert!(s.compose.as_ref().unwrap().quote.is_some());
    assert_eq!(s.compose.as_ref().unwrap().attachments.len(), 2);
    run(&mut s, "compose.redo", json!({}));
    assert_eq!(s.compose.as_ref().unwrap().attachments.len(), 1);
    let saved = run(&mut s, "compose.saveDraft", json!({}))["draft"].as_u64().unwrap();
    run(&mut s, "compose.close", json!({}));
    run(&mut s, "mail.editDraft", json!({"id":saved}));
    assert_eq!(s.compose.as_ref().unwrap().pictures.len(), 1);
    assert_eq!(s.compose.as_ref().unwrap().body.text().pictures().len(), 2);
}
