// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use egui::Key;
use egui_kittest::kittest::Queryable;
use mailcraft_model::{Address, Body, Message};
use serde_json::json;

use super::tests::{app, key, window};

#[test]
fn edit_original_activates_selection_in_the_body_for_delete_and_undo() {
    for theme in ["light", "dark"] {
        let mut app = app();
        app.execute("ui.theme", json!({"value":theme})).unwrap();
        let id = app
            .session
            .mailbox
            .add_message(Message {
                account: app.session.mailbox.accounts[0].id,
                folder: app.session.mailbox.inbox().unwrap(),
                from: Address::new("Ada", "ada@example.org"),
                body: Body { text: "Original passage".into(), html: Some("<p><b>Original passage</b></p>".into()) },
                ..Default::default()
            })
            .unwrap();
        app.execute("mail.forward", json!({"id":id})).unwrap();
        app.execute("compose.set", json!({"body":"My introduction"})).unwrap();
        let mut harness = window(app);
        harness.get_by_label("Edit Original").click();
        harness.run_steps(3);
        let compose = harness.state().session.compose.as_ref().unwrap();
        assert!(compose.quote.is_none());
        assert!(!compose.body.selection().is_empty());
        key(&mut harness, Key::Delete);
        let compose = harness.state().session.compose.as_ref().unwrap();
        assert!(compose.body.plain().contains("My introduction"));
        assert!(!compose.body.plain().contains("Original passage"));
        harness.state_mut().execute("compose.undo", json!({})).unwrap();
        assert!(harness.state().session.compose.as_ref().unwrap().body.plain().contains("Original passage"));
        harness.state_mut().execute("compose.undo", json!({})).unwrap();
        assert!(harness.state().session.compose.as_ref().unwrap().quote.is_some());
    }
}
