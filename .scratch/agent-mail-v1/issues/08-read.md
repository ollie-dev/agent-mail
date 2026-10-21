# 08: Read a message

**What to build:** An Agent reads a message by its reference. Output: headers, the plain-text body (from the text part, or the HTML part converted to text) and an attachment list (name, type, size). Reading uses a peek so the message stays unread. Bodies are truncated at 20,000 characters with a marker unless `--full` is given. A reference whose UIDVALIDITY no longer matches fails instead of reading the wrong message.

**Blocked by:** 07

**Status:** done

- [ ] Reading a plain-text message returns its headers and body
- [ ] Reading an HTML-only message returns readable text without markup
- [ ] A multipart message with attachments lists each attachment's name, type and size, and returns no attachment content
- [ ] After reading, the message is still unread on the fake server
- [ ] A body longer than 20,000 characters is cut at 20,000 with a truncation marker; `--full` returns it complete
- [ ] A reference with a UIDVALIDITY different from the server's fails with a structured `stale_reference` error
- [ ] Reading from a mailbox without a `read` Grant is denied and audited
- [ ] Each read appends an audit entry with the message reference and no body content
