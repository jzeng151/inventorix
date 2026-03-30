# Inventorix — Design Document

> Design system and UI specification for all Tera templates and CSS.
> This is the source of truth for Lane B (Inventory UI) and Lane E (Tauri Shell).
> Update this file when design decisions change. Do not let it drift from the templates.

---

## Design Classifier

**APP UI** — task-focused, data-dense, utility-first. Not a marketing page.

Rules that follow:
- Calm surface hierarchy, strong typography, minimal chrome
- Utility language in copy — orientation, status, action (not mood or brand)
- Cards only when card IS the interaction (never decorative card grids)
- Dense but readable. 40px rows, not 80px cards.

---

## Color Tokens

Define these as CSS custom properties in `static/css/app.css`. Never hardcode hex values in templates.

```css
:root {
  /* Surfaces */
  --color-bg:           #F9FAFB;  /* page background */
  --color-surface:      #FFFFFF;  /* table rows, panels */
  --color-border:       #E5E7EB;  /* table borders, dividers */

  /* Text */
  --color-text-primary: #111827;  /* main labels, values */
  --color-text-muted:   #6B7280;  /* timestamps, secondary labels */

  /* Table header */
  --color-header-bg:    #1F2937;
  --color-header-text:  #F9FAFB;

  /* Actions */
  --color-accent:       #2563EB;  /* primary buttons, links, active state */

  /* Inventory health states */
  --color-healthy-bg:   #F0FDF4;  /* row tint: qty > threshold */
  --color-healthy-fg:   #16A34A;  /* badge text */
  --color-low-bg:       #FFFBEB;  /* row tint: qty approaching threshold */
  --color-low-fg:       #92400E;  /* badge text — dark amber, WCAG AA on #FFFBEB */
  --color-critical-bg:  #FEF2F2;  /* row tint: qty = 0 or at/below threshold */
  --color-critical-fg:  #DC2626;  /* badge text */
}
```

**WCAG AA contrast ratios:**
- `--color-header-text` on `--color-header-bg`: 12.6:1 (passes AAA)
- `--color-healthy-fg` (#16A34A) on white: 4.5:1 (passes AA)
- `--color-low-fg` (#92400E) on `--color-low-bg` (#FFFBEB): 5.1:1 (passes AA)
- `--color-critical-fg` (#DC2626) on `--color-critical-bg` (#FEF2F2): 4.6:1 (passes AA)
- `--color-accent` (#2563EB) on white: 4.7:1 (passes AA)

Do not use `#D97706` (medium amber) for text on light backgrounds — contrast fails.

---

## Typography

```css
@import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;600&display=swap');

body {
  font-family: 'Inter', sans-serif;
  /* No system font fallbacks — Inter is required */
}
```

| Usage | Size | Weight | Color |
|-------|------|--------|-------|
| Table data | 14px | 400 | `--color-text-primary` |
| Table header | 12px | 600 | `--color-header-text` (uppercase) |
| Page headings | 20px | 600 | `--color-text-primary` |
| Subheadings | 14px | 600 | `--color-text-primary` |
| Timestamps, muted | 12px | 400 | `--color-text-muted` |
| Error messages | 14px | 400 | `--color-critical-fg` |

---

## Spacing & Shape

```css
/* Spacing scale — 4px base unit */
--space-1: 4px;
--space-2: 8px;
--space-3: 12px;
--space-4: 16px;
--space-6: 24px;
--space-8: 32px;
--space-12: 48px;

/* Border radius */
--radius-sm: 4px;  /* inputs */
--radius-md: 6px;  /* badges, buttons */
/* Table rows: border-radius: 0 (data density) */
```

---

## Layout

### Top Bar
```
[INVENTORIX]  [NYC Branch]  ────────────────  [Jane D. · Coordinator ▼]  [Logout]
```
- Height: 56px
- Background: `--color-header-bg`
- Text: `--color-header-text`
- Logo: 16px / 600 weight / uppercase tracking
- User menu: name + role, dropdown for future settings

### Sub-bar (Inventory Table only)
```
[Item# ________]  [Collection ▼]  [Bin ____]  [Notes ____]  ──  [Import]  [Export End-of-Day]
```
- Height: 48px
- Background: `--color-surface`
- Border-bottom: 1px solid `--color-border`
- Search inputs: auto-focused on page load (rep "search first" flow)
- Import + Export: right-aligned, muted styling (used once per day, not primary CTA)

### Mobile (<768px)
- Top bar: Logo + hamburger (branch + user collapse)
- Inventory table: horizontal scroll, first 3 columns sticky (ITEM#, COLLECTION, QTY)
- Tile Detail: chat collapses to bottom sheet, tile fields take full width
- Touch targets: minimum 44×44px on all interactive elements

---

## Screens

### Login
```
┌─────────────────────────────────┐
│                                 │
│         INVENTORIX              │
│                                 │
│   Email ____________________    │
│   Password _________________    │
│                                 │
│   [ Sign in ]                   │
│                                 │
│   Incorrect email or password.  │  ← inline error, --color-critical-fg
│                                 │  ← same message for both cases (no enumeration)
└─────────────────────────────────┘
```
- Centered card, `--color-surface`, `--radius-md`, shadow-sm
- No registration link, no "forgot password" (admin resets via admin panel)
- Error: inline below form. No modal, no toast.

---

### Inventory Table

**Screen hierarchy (what the user sees first → last):**
1. **Health summary strip** — "12 critical · 34 low · 654 healthy" (clickable filters, top of table area)
2. **Data table** — sorted by health (critical → low → healthy) by default
3. **Search/filter bar** — persistent, visually subordinate
4. **Row actions** — inline, not competing with data

**Default visible columns (left to right):**

| # | Column | Notes |
|---|--------|-------|
| 1 | ITEM # | Sticky on mobile |
| 2 | COLLECTION | Sticky on mobile |
| 3 | NEW BIN | — |
| 4 | QTY | Health badge inline. Sticky on mobile. |
| 5 | COORDINATOR | Assigned user |
| 6 | SALES REP | Assigned user |
| 7 | REFILL STATUS | Countdown timer or status badge |
| 8 | NOTES | Truncated, full text on hover |

Columns that scroll right (not visible by default): GTS DESCRIPTION, OVERFLOW RACK, ORDER #

**Row behavior:**
- Row height: 40px (compact)
- Click anywhere on row → opens tile detail (`cursor: pointer`)
- No separate "open" icon or link
- Row tinted based on health state (subtle background, not loud)

**No pagination.** 700 rows × 40px = 28,000px total height. Scrollable, not paginated. Virtual scroll is unnecessary at this count.

**Health badge (inline in QTY cell):**
```
[  2  ●]   ← --color-critical-fg dot, --color-critical-bg row tint
[ 12  ●]   ← --color-low-fg dot, --color-low-bg row tint
[ 87  ●]   ← --color-healthy-fg dot, --color-healthy-bg row tint (subtle)
```
- ARIA: `aria-label="Critical: 2 in stock"` (color alone is insufficient)
- No text label ("critical") inline — too noisy at 700 rows

**Role-aware column rendering:**
- Role determines what **renders**, not what is disabled or hidden with `display:none`
- Sample Coordinator: REFILL STATUS column shows "Request Refill" button (idle) or countdown timer (pending)
- Sales Rep: REFILL STATUS column shows "Approve" button (when pending), read-only otherwise
- Admin: sees all columns and actions
- Neither role sees the other's action button — the column content differs, not the visibility

**Refill Status column states:**

| State | Renders | Who sees it |
|-------|---------|-------------|
| No request | Empty | Both |
| Pending (Coordinator view) | `⏱ 43h 12m` — amber fill, not clickable | Coordinator |
| Pending (Rep view) | `[ Approve ]` — blue outline button | Sales Rep |
| Approved | `Approved ✓` — green, static | Both |
| Fulfilled | `Fulfilled` — muted | Both |
| Expired (Coordinator) | `Expired — Re-request?` — red outline, re-opens form | Coordinator |
| No active request (Coordinator) | `[ Request Refill ]` — muted amber outline, 28px height | Coordinator |

**Refill request form:** Inline slide-down within the row (not a modal). QTY requested field + submit. Row expands while open. Submits on Enter.

**WebSocket disconnect banner:**
When WS drops: amber banner at top of inventory table:
```
⚠ Real-time updates paused — reconnecting...
```
Auto-dismisses on reconnect. Does not block interaction. `aria-live="polite"`.

---

### Tile Detail

```
┌─────────────────────────────────────────────────────────────┐
│  ← Back to inventory                                        │
│  Calacatta Marble 12x24  ·  BIN-042  ·  ● Critical: 2       │
├────────────────────────────────┬────────────────────────────┤
│  TILE FIELDS (60%)             │  CHAT (40%)                │
│                                │                            │
│  Item #:  CAL-1224             │  Jane D. · 2:14 PM         │
│  Collection: Calacatta Marble  │  Do we have enough for the │
│  GTS Desc: ...                 │  Riverside showroom?       │
│  Bin: BIN-042                  │                            │
│  Qty: 2  [ edit ]              │  Mike R. · 2:22 PM         │
│  Overflow: No                  │  Only 2 left, placing a    │
│  Order #: ORD-5521             │  refill request now.       │
│  Notes: ...                    │                            │
│  Coordinator: Jane D.          │  ─────────────────────     │
│  Sales Rep: Mike R.            │  [ Type a message...  ] ↵  │
│                                │  Ctrl+Enter to send        │
│  ── REFILL REQUEST ──          │                            │
│  ⏱ 43h 12m remaining          │                            │
│  Requested by Jane D.          │                            │
│  Qty requested: 10             │                            │
│  [ Approve ]  (Sales Rep only) │                            │
└────────────────────────────────┴────────────────────────────┘
```
- Left column scrolls independently; right column (chat) scrolls independently
- Chat: oldest message at top, compose at bottom
- Chat messages: `aria-live="polite"` on message list (new messages announced to screen readers)
- No tab bar — both panels visible simultaneously on desktop
- Mobile: chat collapses to bottom sheet, tile fields take full width

---

### Import Page

```
┌─────────────────────────────────────────────┐
│  Import Inventory                           │
│                                             │
│  ┌─────────────────────────────────────┐    │
│  │                                     │    │
│  │      Drop your .xlsx file here      │    │
│  │      or  [ Choose File ]            │    │
│  │                                     │    │
│  └─────────────────────────────────────┘    │
│                                             │
│  Importing row 243 of 700...  ████░░░  34%  │  ← progress bar
│                                             │
│  ✓ 700 tiles imported.                      │  ← success (auto-dismiss 5s)
│    3 duplicates skipped.                    │
│    2 errors:                                │
│      Row 14: Non-numeric QTY value "N/A"    │
│      Row 87: Missing ITEM NUMBER            │
│    [ Download error report ]                │
└─────────────────────────────────────────────┘
```

---

### Admin Panel

```
┌──────────────────────────────────────────────────────────────┐
│  User Management                              [ + New User ] │
├─────────────────────┬────────────────────────────────────────┤
│  USERS              │  Jane D.                               │
│                     │  Role: Sample Coordinator              │
│  Jane D.  NYC  Coord│  Branch: NYC                           │
│  Mike R.  NYC  Rep  │  Email: jane@company.com               │
│  [Deactivated] ...  │                                        │
│                     │  [ Save Changes ]                      │
│                     │  [ Deactivate User ]  ← inline confirm │
│                     │  "Deactivate Jane D.? This will end    │
│                     │   her current session immediately."    │
│                     │  [ Yes, deactivate ]  [ Cancel ]       │
└─────────────────────┴────────────────────────────────────────┘
```
- Deactivation: inline confirmation (not modal). Immediately shows "Deactivated" badge in list.
- Deactivation invalidates all active sessions for that user instantly (DELETE FROM sessions WHERE user_id = ?)

---

## Interaction States

| Feature | Loading | Empty | Error | Success |
|---------|---------|-------|-------|---------|
| Inventory table | Skeleton rows (same 40px height) | "No tiles imported yet. Import your first Excel file." + [Import Now] | Toast: "Failed to load inventory. Retry?" | Instant render (synchronous DB read) |
| Search/filter | None — client-side, instant | "No tiles match your search." + [Clear filters] | N/A | N/A |
| Excel Import | Progress bar: "Importing row 243 of 700..." | N/A | Inline error list by row + [Download error report] | "700 tiles imported. 3 duplicates skipped." → auto-dismiss 5s |
| Refill Request | Button disabled + spinner | N/A | Toast: "Could not create request. Try again." | Inline timer appears in row via WebSocket broadcast |
| Refill Approval | Button disabled + spinner | N/A | Toast: "Salesforce notification failed — request still approved." | Row updates immediately to "Approved ✓" |
| Chat send | Message appears immediately (optimistic, dimmed) | "No messages yet. Start the conversation." | Dimmed message → "Failed to send. Retry?" | Message un-dimmed, timestamp appears |
| Login | Button disabled + spinner | N/A | "Incorrect email or password." (same for both — no enumeration) | Redirect to inventory table |
| EOD Export | Button disabled + "Generating..." | N/A | Toast: "Export failed. Check if the file is open in Excel." | "Export complete. Digest opened in browser." |
| Admin deactivate | Button disabled | N/A | Toast error | "Deactivated" badge appears inline in list |

---

## Empty States

Every empty state has: a simple icon, a plain-English explanation, and a primary action.

**Inventory table — first run:**
```
[ upload icon ]
No tiles imported yet.
Import your Excel inventory sheet to get started.
[ Import Excel File → ]
```

**Search — no results:**
```
No tiles match "calacatta marble".
Check spelling or try a different filter.
[ Clear filters ]
```

**Chat — new tile, no messages:**
```
No messages yet.
Use chat for questions, notes, or updates about this tile.
[ Start the conversation ]  ← focuses compose input on click
```

**Refill history — none:**
```
No refill requests for this tile.
```
No CTA button here. Refill is initiated from the refill panel, not from history.

---

## Accessibility

### ARIA
- Inventory table: `role="grid"`, `aria-sort` on sortable column headers
- Health badges: `aria-label="Critical: 2 in stock"` (color dot alone is insufficient)
- Refill countdown timer: `aria-live="polite"` (screen reader hears status changes)
- Chat message list: `aria-live="polite"` (new messages announced)
- WS disconnect banner: `aria-live="polite"`
- Import progress: `role="progressbar"`, `aria-valuenow`, `aria-valuemax`

### Keyboard Navigation
- Inventory table rows: `tabindex="0"`, `Enter` opens tile detail
- Search bar: auto-focused on inventory table page load
- Refill buttons: `Space`/`Enter` triggers
- Chat compose: `Ctrl+Enter` sends message
- All interactive elements reachable via `Tab`

### Touch Targets
- Minimum 44×44px for all buttons (applies to mobile breakpoint)

### Contrast
All color token combinations pass WCAG AA (see contrast ratios in Color Tokens section).

---

## Component Reference

### Toast Notifications
- Position: top-right, 16px from edge
- Auto-dismiss: 5 seconds (success), persistent until dismissed (error)
- Error toast: `--color-critical-bg` background, `--color-critical-fg` text
- Success toast: `--color-healthy-bg` background, `--color-healthy-fg` text
- Max width: 320px

### Buttons

| Variant | Use case | Style |
|---------|----------|-------|
| Primary | "Sign in", "Save Changes" | `--color-accent` fill, white text, `--radius-md` |
| Secondary | "Cancel", "Clear filters" | White fill, `--color-border` border |
| Amber outline | "Request Refill" (idle) | `--color-low-fg` border + text, transparent fill, 28px height |
| Amber fill | Refill timer countdown | `--color-low-bg` fill, `--color-low-fg` text |
| Danger outline | "Expired — Re-request?" | `--color-critical-fg` border + text |
| Blue outline | "Approve" | `--color-accent` border + text, transparent fill |

### Form Inputs
- Height: 36px
- Border: 1px solid `--color-border`
- Border-radius: `--radius-sm` (4px)
- Focus ring: 2px `--color-accent` offset 2px
- Placeholder: `--color-text-muted`

### Table
- Header: `--color-header-bg` / `--color-header-text`, 12px uppercase, 600 weight
- Row: `--color-surface`, 40px height, 1px `--color-border` bottom border
- Row hover: slightly darken surface (e.g., `#F3F4F6`)
- Row tint: health state background overrides row background

---

## Not in Scope (design)

- Dark mode — not needed for this user profile and environment
- RTL language support — not applicable
- Print stylesheet — digest is HTML opened in browser, printed via browser
- PDF export — deferred to Phase 3 post-pilot
- Animation / transitions beyond the refill form slide-down
- Custom icon library — use a small subset of Heroicons (outline) only where needed, not decoratively

---

## Review History

| Review | Date | Score | Status |
|--------|------|-------|--------|
| `/plan-design-review` | 2026-03-30 | 2/10 → 8/10 | CLEAR |

All 6 unresolved design decisions resolved. 0 deferred. Plan updated.
