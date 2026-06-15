# Event Restreamer Languages Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-language assignment to event restream coordinators, mirroring the existing game restreamer model; existing event restreamers get all languages; UI shows only checkboxes for languages the event has effective role bindings for.

**Architecture:** Add a `language` column to the `restreamers` table making the primary key `(series, event, restreamer, language)`. Keep `Data::restreamers()` returning a distinct `Vec<User>` (unchanged callers in `roles.rs`). Add `Data::restreamers_with_languages()` returning grouped `Vec<(User, Vec<Language>)>` for the configure UI. Fetch `EffectiveRoleBinding::for_event` in `restreamers_form()` to determine which language checkboxes to render.

**Tech Stack:** Rust, Rocket, sqlx, maud `html!` macros, PostgreSQL, existing `Language` enum in `src/lang.rs`

---

### Task 1: Migration — add language to restreamers

**Files:**
- Create: `migrations/070_event_restreamer_languages.sql`

- [ ] **Step 1: Write the migration**

```sql
-- Add language column; default 'en' temporarily so existing rows get a value
ALTER TABLE restreamers ADD COLUMN language language NOT NULL DEFAULT 'en';

-- Copy existing rows for the other three languages
INSERT INTO restreamers (series, event, restreamer, language)
SELECT series, event, restreamer, unnest(ARRAY['fr'::language, 'de'::language, 'pt'::language])
FROM (SELECT DISTINCT series, event, restreamer FROM restreamers WHERE language = 'en') existing;

-- Add primary key on the full tuple (replaces the implicit uniqueness the app enforced)
ALTER TABLE restreamers ADD PRIMARY KEY (series, event, restreamer, language);

-- Drop the default — language must be explicit going forward
ALTER TABLE restreamers ALTER COLUMN language DROP DEFAULT;
```

- [ ] **Step 2: Apply the migration**

```bash
DATABASE_URL="postgres://mido@127.0.0.1/midos-house" sqlx migrate run
```

Expected: migration `070_event_restreamer_languages` applied successfully.

- [ ] **Step 3: Commit**

```bash
git add migrations/070_event_restreamer_languages.sql
git commit -m "feat: add language column to event restreamers table"
```

---

### Task 2: Add `restreamers_with_languages()` to `Data` in `src/event/mod.rs`

**Files:**
- Modify: `src/event/mod.rs` (near line 684, alongside the existing `restreamers()`)

Context: `restreamers()` (line 684) returns `Vec<User>` by querying `SELECT restreamer FROM restreamers WHERE series = $1 AND event = $2`. With the new schema that query now returns one row per language per user — add `DISTINCT` so it still returns one `User` per coordinator. Then add a new method for the grouped form display.

- [ ] **Step 1: Add DISTINCT to existing `restreamers()` query**

In `src/event/mod.rs`, change the query inside `restreamers()` (line 686):

```rust
for id in sqlx::query_scalar!(r#"SELECT DISTINCT restreamer AS "restreamer: Id<Users>" FROM restreamers WHERE series = $1 AND event = $2"#, self.series as _, &self.event).fetch_all(&mut **transaction).await? {
```

(Only the SQL string changes — add `DISTINCT`.)

- [ ] **Step 2: Add `restreamers_with_languages()` method**

After the closing `}` of `restreamers()` (around line 692), insert:

```rust
pub(crate) async fn restreamers_with_languages(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<Vec<(User, Vec<Language>)>, Error> {
    let rows = sqlx::query!(
        r#"SELECT restreamer AS "restreamer: Id<Users>", language AS "language: Language" FROM restreamers WHERE series = $1 AND event = $2 ORDER BY restreamer, language"#,
        self.series as _,
        &self.event
    ).fetch_all(&mut **transaction).await?;

    let mut grouped: indexmap::IndexMap<Id<Users>, (User, Vec<Language>)> = indexmap::IndexMap::new();
    for row in rows {
        let entry = grouped.entry(row.restreamer).or_insert_with(|| {
            // User lookup happens below; placeholder until we resolve
            (User { id: row.restreamer, ..Default::default() }, Vec::new())
        });
        entry.1.push(row.language);
    }

    // Resolve User structs (replaces placeholders)
    let mut result = Vec::new();
    for (id, (_, langs)) in grouped {
        let user = User::from_id(&mut **transaction, id).await?.ok_or(Error::RestreamerUserData)?;
        result.push((user, langs));
    }
    // Sort by display name for stable ordering
    result.sort_by(|(a, _), (b, _)| a.display_name().cmp(b.display_name()).then_with(|| a.id.cmp(&b.id)));
    Ok(result)
}
```

Note: `User` may not implement `Default`. Use a different grouping approach that avoids the placeholder:

```rust
pub(crate) async fn restreamers_with_languages(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<Vec<(User, Vec<Language>)>, Error> {
    // Collect (id, language) pairs first
    let rows = sqlx::query!(
        r#"SELECT DISTINCT restreamer AS "restreamer: Id<Users>", language AS "language: Language" FROM restreamers WHERE series = $1 AND event = $2 ORDER BY restreamer, language"#,
        self.series as _,
        &self.event
    ).fetch_all(&mut **transaction).await?;

    // Group languages by user id
    let mut by_id: std::collections::BTreeMap<Id<Users>, Vec<Language>> = std::collections::BTreeMap::new();
    for row in rows {
        by_id.entry(row.restreamer).or_default().push(row.language);
    }

    // Resolve user structs
    let mut result = Vec::new();
    for (id, langs) in by_id {
        let user = User::from_id(&mut **transaction, id).await?.ok_or(Error::RestreamerUserData)?;
        result.push((user, langs));
    }
    result.sort_by(|(a, _), (b, _)| a.display_name().cmp(b.display_name()).then_with(|| a.id.cmp(&b.id)));
    Ok(result)
}
```

- [ ] **Step 3: Cargo check**

```bash
DATABASE_URL="postgres://mido@127.0.0.1/midos-house" cargo check 2>&1 | head -60
```

Expected: no errors in `event/mod.rs`.

- [ ] **Step 4: Commit**

```bash
git add src/event/mod.rs
git commit -m "feat: add restreamers_with_languages() and DISTINCT to Data::restreamers()"
```

---

### Task 3: Update configure.rs — form struct, display, and handlers

**Files:**
- Modify: `src/event/configure.rs` (lines ~409–700)

Changes:
1. `AddRestreamerForm` gets a `languages: Vec<Language>` field
2. `restreamers_form()` fetches effective bindings → active languages; shows languages column; adds language checkboxes
3. `add_restreamer` handler: validate ≥1 language; insert one row per language; remove the "already a coordinator" contains check (INSERT ... ON CONFLICT DO NOTHING is idempotent)
4. `copy_restreamers` SQL includes the `language` column

- [ ] **Step 1: Add `languages` field to `AddRestreamerForm`**

At line ~562, change:

```rust
#[derive(FromForm, CsrfForm)]
pub(crate) struct AddRestreamerForm {
    #[field(default = String::new())]
    csrf: String,
    restreamer: String,
    #[field(default = Vec::new())]
    languages: Vec<Language>,
}
```

- [ ] **Step 2: Update `restreamers_form()` to fetch active languages and display them**

At the top of `restreamers_form()`, after `let restreamers = event.restreamers(&mut transaction).await?;`, replace with:

```rust
let restreamers_with_langs = event.restreamers_with_languages(&mut transaction).await?;
```

And fetch active languages (add after `let is_elevated = ...`):

```rust
let effective_bindings = roles::EffectiveRoleBinding::for_event(&mut transaction, event.series, &event.event).await?;
let active_languages = roles::EffectiveRoleBinding::active_languages(&effective_bindings, event.default_volunteer_language);
```

Then update the table section — add a "Languages" `<th>` and render the coordinator's languages. Change the loop from `@for restreamer in restreamers` to `@for (restreamer, langs) in &restreamers_with_langs`:

```rust
@if restreamers_with_langs.is_empty() {
    p : "No restream coordinators so far.";
} else {
    table {
        thead {
            tr {
                th : "Restream coordinator";
                th : "Languages";
                th;
            }
        }
        tbody {
            @for (restreamer, langs) in &restreamers_with_langs {
                tr {
                    td : restreamer;
                    td {
                        : langs.iter().map(|l| l.short_code().to_uppercase()).collect::<Vec<_>>().join(", ");
                    }
                    td {
                        @let errors = defaults.remove_errors(restreamer.id);
                        @let (errors, button) = button_form(uri!(remove_restreamer(event.series, &*event.event, restreamer.id)), csrf, errors, "Remove");
                        : errors;
                        div(class = "button-row") : button;
                    }
                }
            }
        }
    }
}
```

Then add language checkboxes to the Add form (after the `restreamer` field):

```rust
: form_field("languages", &mut errors, html! {
    label : "Languages:";
    @for lang in &active_languages {
        label {
            input(type = "checkbox", name = "languages", value = lang.short_code());
            : " ";
            : lang;
        }
    }
    label(class = "help") : "Select which languages this coordinator will handle. Only languages with active role bindings are shown.";
});
```

- [ ] **Step 3: Update `add_restreamer` handler**

Replace the block starting at the `if let Some(ref value) = form.value` check (line ~575). Key changes:

a) After user existence check, validate languages not empty:

```rust
if value.languages.is_empty() {
    form.context.push_error(form::Error::validation("Please select at least one language.").with_name("languages"));
}
```

b) Remove the `data.restreamers(...).contains(&restreamer)` check entirely — INSERT ON CONFLICT DO NOTHING handles idempotency.

c) Replace the single INSERT with a per-language loop:

```rust
// was: sqlx::query!("INSERT INTO restreamers (series, event, restreamer) VALUES ($1, $2, $3)", ...)
for &lang in &value.languages {
    sqlx::query!(
        "INSERT INTO restreamers (series, event, restreamer, language) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        data.series as _,
        &data.event,
        restreamer_id as _,
        lang as _
    ).execute(&mut *transaction).await?;
}
```

Full updated handler block (only the non-error path changes):

```rust
if form.context.errors().next().is_some() {
    RedirectOrContent::Content(restreamers_form(transaction, Some(me), uri, csrf.as_ref(), data, RestreamersFormDefaults::AddContext(form.context)).await?)
} else {
    for &lang in &value.languages {
        sqlx::query!(
            "INSERT INTO restreamers (series, event, restreamer, language) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
            data.series as _,
            &data.event,
            restreamer_id as _,
            lang as _
        ).execute(&mut *transaction).await?;
    }
    transaction.commit().await?;
    RedirectOrContent::Redirect(Redirect::to(uri!(restreamers_get(series, event))))
}
```

- [ ] **Step 4: Update `copy_restreamers` SQL**

At line ~689, change the INSERT SQL to include the `language` column:

```rust
sqlx::query!(
    "INSERT INTO restreamers (series, event, restreamer, language) SELECT $1, $2, restreamer, language FROM restreamers WHERE series = $3 AND event = $4 ON CONFLICT DO NOTHING",
    data.series as _, &data.event, source_series as _, &source_event_slug
).execute(&mut *transaction).await?;
```

- [ ] **Step 5: Fix imports if needed**

`EffectiveRoleBinding` and `roles` must be accessible in `configure.rs`. Check top of file:

```bash
grep -n "^use\|^mod\|roles" /Users/ch.wergen/source/priv/hyrule-town-hall/src/event/configure.rs | head -20
```

If `roles` isn't imported, add the relevant `use` or call it as `super::roles::EffectiveRoleBinding::for_event(...)` (since `configure.rs` is in `src/event/`).

- [ ] **Step 6: Cargo check**

```bash
DATABASE_URL="postgres://mido@127.0.0.1/midos-house" cargo check 2>&1 | head -80
```

Expected: no errors. Fix any type mismatches (e.g. `&active_languages` vs `Vec<Language>` ownership, `langs` display in maud).

- [ ] **Step 7: Commit**

```bash
git add src/event/configure.rs
git commit -m "feat: show language checkboxes and assignment for event restream coordinators"
```

---

### Task 4: Final cargo check and cleanup

**Files:** No new files.

- [ ] **Step 1: Full cargo check**

```bash
DATABASE_URL="postgres://mido@127.0.0.1/midos-house" cargo check 2>&1
```

Expected: zero errors. If any errors remain, fix them now.

- [ ] **Step 2: Verify `roles.rs` callers unchanged**

The three `data.restreamers(...).await?.contains(&me)` calls in `src/event/roles.rs` at lines 3160, 3413, and 4114 must still compile. The `DISTINCT` change in Task 2 Step 1 ensures `restreamers()` still returns `Vec<User>` — `contains` on `Vec<User>` uses `PartialEq<User>` which hasn't changed.

```bash
grep -n "data.restreamers" /Users/ch.wergen/source/priv/hyrule-town-hall/src/event/roles.rs
```

Expected: still 3 hits, all calling `.contains(...)`.

- [ ] **Step 3: Commit if any fixups were needed**

```bash
git add -p
git commit -m "fix: resolve compile errors from event restreamer language feature"
```
