// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Opt-in editing of the original carried under a reply or forward.

use std::collections::{HashMap, HashSet};

use mailcraft_model::{
    pictures,
    rich::{Paragraph, RichText},
};
use serde_json::{Value, json};

use super::super::{fail, needs_compose};
use crate::{CmdResult, Session, p};

pub(crate) const EDIT_QUOTE_TAG: u64 = 1;
const MAX_EDIT_HTML: usize = 256 * 1024;
const MAX_IMAGES: usize = 4096;

pub(super) fn enabled(s: &Session) -> Option<&'static str> {
    needs_compose(s).or_else(|| s.compose.as_ref().and_then(|c| c.quote.is_none().then_some("the message has no original to edit")))
}

pub(super) fn remove(s: &mut Session, v: &Value) -> CmdResult {
    p::only(v, &[])?;
    let c = s.compose.as_mut().ok_or_else(|| fail("no message is being written"))?;
    let quote = c.quote.as_ref().ok_or_else(|| fail("the message has no original to edit"))?;
    let ids = mailcraft_model::shown::shown_ids(&quote.html);
    c.quote_edit_attachments = c.attachments.iter().filter(|part| mailcraft_model::shown::shown_in_place(&ids, part)).cloned().collect();
    c.attachments.retain(|part| !c.quote_edit_attachments.contains(part));
    c.body.push_tag(EDIT_QUOTE_TAG);
    c.quote_edit_history = c.quote.take();
    c.dirty = true;
    Ok(json!({"removed": true}))
}

pub(super) fn edit(s: &mut Session, v: &Value) -> CmdResult {
    p::only(v, &["omitUnavailablePictures"])?;
    let omit_pictures = p::bool(v, "omitUnavailablePictures").unwrap_or(false);
    let c = s.compose.as_ref().ok_or_else(|| fail("no message is being written"))?;
    let quote = c.quote.as_ref().ok_or_else(|| fail("the message has no original to edit"))?;
    if quote.html.len() > MAX_EDIT_HTML {
        return Err(fail("the original is too large to convert to editable text"));
    }
    let blocks = mailcraft_html::parse_text(&quote.html);
    if blocks.len() >= mailcraft_html::MAX_BLOCKS {
        return Err(fail("the original is too large to convert to editable text"));
    }
    let mut omitted = mailcraft_html::bg_pictures(&blocks).len()
        + mailcraft_html::images(&blocks).iter().filter(|block| {
            matches!(&block.kind, mailcraft_html::BlockKind::Image { src, .. } if !src.get(..4).is_some_and(|scheme| scheme.eq_ignore_ascii_case("cid:")))
        }).count();
    if omitted > 0 && !omit_pictures {
        return Err(fail("the original has pictures that cannot be kept in editable text"));
    }
    let mut parts = quote.pictures.clone();
    parts.extend(c.attachments.iter().cloned());
    let mut kept = c.pictures.clone();
    let mut seq = c.picture_seq;
    let held: HashSet<_> = c.body.text().pictures().into_iter().collect();
    let mut found = HashMap::<String, Option<u32>>::new();
    let mut looked = 0;
    let mut missing = false;
    let mut consumed = HashSet::new();
    let fragment = mailcraft_mime_model::rich::from_html_for_editing(&quote.html, &mut |img| {
        looked += 1;
        if looked > MAX_IMAGES {
            missing = true;
            return None;
        }
        let key = img.cid.to_ascii_lowercase();
        if let Some(id) = found.get(&key) {
            omitted += usize::from(id.is_none());
            return *id;
        }
        let part = mailcraft_model::shown::part_for(&parts, &format!("cid:{}", img.cid));
        let id = part
            .filter(|(_, part)| part.data.len() <= pictures::MAX_PICTURE_BYTES)
            .and_then(|(_, part)| pictures::add(&mut kept, &mut seq, &held, &part.name, part.data.to_vec(), Some(&img.alt), img.width).ok());
        missing |= id.is_none();
        omitted += usize::from(id.is_none());
        if id.is_some()
            && let Some((_, part)) = part
        {
            consumed.insert(part.content_id.clone());
        }
        found.insert(key, id);
        id
    });
    if looked > MAX_IMAGES || (missing && !omit_pictures) {
        return Err(fail("some original pictures cannot be kept in editable text; the original is still intact"));
    }
    if fragment.is_empty() {
        return Err(fail("the original has no editable text"));
    }
    // Joining happens on a temporary document. Even a full body refuses the conversion atomically.
    let mut text = c.body.text().clone();
    let at = text.len();
    let mut paragraphs = vec![Paragraph::default(), Paragraph::default()];
    paragraphs.extend(fragment.paragraphs().iter().cloned());
    let fragment = RichText::from_paragraphs(paragraphs);
    text.insert_rich(at, &fragment).map_err(|e| fail(e.to_string()))?;
    let end = text.len();
    let c = s.compose.as_mut().ok_or_else(|| fail("no message is being written"))?;
    c.body.replace(text).map_err(|e| fail(e.to_string()))?;
    c.body.tag_edit(EDIT_QUOTE_TAG);
    c.body.select(at + 2, end).map_err(|e| fail(e.to_string()))?;
    c.quote_edit_history = c.quote.take();
    c.quote_edit_attachments = c.attachments.iter().filter(|part| consumed.contains(&part.content_id)).cloned().collect();
    c.attachments.retain(|part| !consumed.contains(&part.content_id));
    c.pictures = kept;
    c.picture_seq = seq;
    c.dirty = true;
    Ok(json!({"from": at + 2, "to": end, "editable": true, "omittedPictures":omitted}))
}
