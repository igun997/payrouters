# PayRouters design handoff

Design file: [PayRouters in Wonder](https://app.wonder.so/indra/files/01a217f3-2261-72de-ba5f-ff4a02b7baa6). One page, 98 artboards in ten labelled groups. Every artboard name ends with its viewport width, so the name is the lookup key (for example `Dashboard Providers 1366`, `Snap VA 320`).

## 1. Viewports and breakpoints

| Range | Name | Tailwind | Reference artboards |
|---|---|---|---|
| 320-359 | XS, smallest supported | default (no prefix) | 320x568 |
| 360-767 | Base phone (sm 640 folded in) | `min-[360px]` / `sm:` | 390x844 |
| 768-1023 | md tablet | `md:` | 768x1024 |
| 1024-1279 | lg small laptop | `lg:` | 1024x768 |
| 1280-1535 | xl desktop, primary | `xl:` | **1366x768** (1440 is the same layout) |
| >= 1536 | 2xl wide | `2xl:` | content column max-w 1280 centred |

Layout switches (these need Tailwind prefixes, everything else is variable-driven):
- Sidebar: none below md (bottom tab bar, 5 tabs), 72px icon rail at md (768-1023), 248px from lg.
- Tables: card lists below md, real tables from md. At md the Provider-and-method column folds under Product.
- Dialogs: bottom sheet below sm (640), centred 480 (forms) or 560 (provider forms) from sm. Forms with 4+ fields on phones are full screens with a sticky action bar. Dialog bodies scroll between a fixed header and footer (`max-h-[70svh]`).
- Snap: full-bleed phone layout below 640; from 640 a centred 480 card on the page background with a slim brand strip.
- Dashboard stat rows: 4 columns only from xl (1280); 1 or 2 columns below lg; **2 columns at md and lg** because "Rp 48.250.000" at 24px needs about 182px and a 4-column card at 1024 only has about 130px. Amounts never wrap onto a second line ("Rp" alone on a line is a defect): if an amount does not fit, reduce columns, not the font.
- Overview chart row: 2/3 + 1/3 (`grid-cols-[1fr_380px]`) at xl, stacked with provider tiles at lg, stacked at md and below.
- Content column: `2xl:mx-auto 2xl:max-w-[1280px]`.

## 2. Type, spacing and size scale (breakpoint level)

Source of truth: artboard `Foundations Breakpoints`. Ready-to-paste implementation: [`docs/responsive.css`](./responsive.css), generated from the table. It defines one CSS variable per style per breakpoint inside a single `@media` block each, and one semantic class per style (`text-amount-xl`, `text-amount-lg`, `text-stat`, `text-result-title`, `text-page-title`, `text-dialog-title`, `text-card-title`, `text-body`, `text-label`, `text-helper`, `text-micro`, `text-mono-id`, `text-mono-va`, `text-cell`, `text-tab`, `text-button`) plus `p-page`, `p-card`, `gap-section`, `p-dialog`, `h-control`. Components use these classes only; no component writes `md:text-…`.

| Style (font-size/line-height) | XS 320 | Base 360 | md 768 | lg 1024 | xl 1280 | 2xl 1536 |
|---|---|---|---|---|---|---|
| amount-xl (Snap choose amount) | 36/40 | 44/48 | 44/48 | 44/48 | 44/48 | 44/48 |
| amount-lg (VA, QRIS, detail amount) | 28/34 | 34/40 | 36/42 | 36/42 | 36/42 | 36/42 |
| stat (dashboard stat figure) | 22/28 | 24/30 | 24/30 | 24/30 | 24/30 | 30/36 |
| result-title | 24/30 | 28/34 | 28/34 | 28/34 | 28/34 | 28/34 |
| page-title | 20/26 | 22/28 | 24/30 | 24/30 | 24/30 | 28/34 |
| dialog-title | 18/24 | 20/26 | 20/26 | 20/26 | 20/26 | 20/26 |
| card-title | 16/22 | 16/22 | 18/24 | 18/24 | 18/24 | 18/24 |
| body | 14/20 | 15/22 | 15/22 | 15/22 | 15/22 | 15/22 |
| label / helper / micro | 14/20, 13/18, 12/16 at every width | | | | | |
| mono-id | 14/20 | 15/20 | 14/20 | 14/20 | 14/20 | 14/20 |
| mono-va | 22/28 | 28/34 | 28/34 | 28/34 | 28/34 | 28/34 |
| table cell | - | - | 14/20 | 14/20 | 14/20 | 14/20 |
| tab label | 12/16 | 13/16 | - | - | - | - |
| button | 15/20 at every width | | | | | |

| Spacing | XS | Base | md | lg | xl | 2xl |
|---|---|---|---|---|---|---|
| page padding | 16 | 20 | 24 | 24 | 32 | 32 |
| card padding | 16 | 20 | 20 | 24 | 24 | 24 |
| section gap | 12 | 16 | 16 | 20 | 20 | 20 |
| dialog padding | 16 (sheet) | 20 (sheet) | 24 | 24 | 24 | 24 |
| control height | 48 | 48 | 44 | 44 | 44 | 44 |

Fonts: Plus Jakarta Sans (UI), JetBrains Mono (ids, keys, VA numbers, URLs, JSON). No tabular-nums in the design tool, so enable `font-variant-numeric: tabular-nums` on figures in code.

## 3. Tokens

Colour, radius, spacing and font tokens live in the Wonder file (artboard `Foundations Tokens`); each token name maps to a Tailwind v4 theme variable (`--surface-page` -> `--color-surface-page` giving `bg-surface-page`).

| Token | Value |
|---|---|
| surface-page / card / sunken / inverse | #F6F8F7 / #FFFFFF / #EDF1EF / #0C1F1A |
| border-default / strong | #DCE3DF / #BFCBC5 |
| content-primary / secondary / tertiary / inverse | #0C1F1A / #43544E / #5B6C66 / #FFFFFF |
| action-primary / hover / on-primary | #0B3B2E / #0F4D3C / #FFFFFF |
| signal-default | #19C37D |
| status success bg/fg | #E2F5EA / #0A7547 |
| status pending bg/fg/dot | #FFF1D0 / #80530A / #F2A900 |
| status failed bg/fg | #FDE7E4 / #B02318 |
| status expired bg/fg | #EDF1EF / #43544E |
| status sandbox bg/fg | #E4ECFF / #1E4DB8 |
| radius sm / md / lg / xl / pill | 8 / 12 / 16 / 24 / 999 |
| space 1,2,3,4,5,6,8 | 4, 8, 12, 16, 20, 24, 32 |

## 4. Controls and rules

Artboard `Foundations Controls` shows every state. Inputs 44 (desktop) / 48 (phone) high, radius 12, 1px `border-strong`; focus = 2px `action-primary` border + 3px `status-success-bg` ring; error = 2px `status-failed-fg` border + icon + message (never colour alone); disabled = `surface-sunken`. Optional fields say Optional at the right of the label. Secrets are write-only: masked as `••••abcd`, helper `Leave blank to keep the saved key`. Buttons: primary (evergreen), secondary (outline), danger (solid red, destructive confirm only), danger-outline, ghost, disabled, loading (spinner, label kept). One primary per view. Banners are persistent and inside content; toasts are transient (4 s), bottom centre on desktop, above the tab bar on phones. Copy values (secrets, keys, URLs) are mono, `break-all`, with a Copy button. Touch targets >= 44 on phones.

## 5. Screen index (artboard -> route -> data)

Admin routes and API: see `docs/api.md`. Snap API: `GET/POST /snap/api/:token[/pay|/simulate]`.

### Foundations: tokens, breakpoints and form controls

- `Foundations Tokens`
- `Foundations Breakpoints`
- `Foundations Controls`

### Snap: customer payment page, phone 390 (Base, 360 to 767)

- `Snap Choose 390`
- `Snap VA 390`
- `Snap IDRX VA 390`
- `Snap QRIS 390`
- `Snap Paid 390`
- `Snap Expired 390`
- `Snap Failed 390`
- `Snap Cancelled 390`
- `Snap Provider Redirect 390`
- `Snap Mandiri Bill 390`
- `Snap Method Error 390` (shared state component)
- `Snap Copied 390`
- `Snap Loading 390`
- `Snap Not Found 390` (shared state component)

### Snap: smallest phone 320 (XS), tablet 768 (md), desktop 1366 (xl)

- `Snap Choose 320`
- `Snap VA 320`
- `Snap QRIS 320`
- `Snap Paid 320`
- `Snap Expired 320`
- `Snap VA 768`
- `Snap Choose 1366`
- `Snap VA 1366`
- `Snap Paid 1366`

### Dashboard: desktop pages 1366x768 (xl)

- `Dashboard Login 1366` (`/login`)
- `Dashboard Login Error 1366` (shared state component)
- `Dashboard Overview 1366` (`/`)
- `Dashboard Payments 1366` (`/payments`)
- `Dashboard Payment Detail 1366` (`/payments/[id]`)
- `Dashboard Cash-ins 1366` (`/cash-ins`)
- `Dashboard Products 1366` (`/products`)
- `Dashboard Providers 1366` (`/providers`)
- `Dashboard Webhooks Inbound 1366`
- `Dashboard Webhooks Outbound 1366`
- `Dashboard Playground 1366` (`/playground`)
- `Dashboard Date Range Popover 1366`
- `Dashboard Live Topbar 1366`

### Dashboard: dialogs and forms, desktop (480 and 560 wide, centred over a scrim)

- `Dashboard Dialog New Product 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Dialog New Product Error 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Dialog Secret Reveal 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Dialog Rotate Secret 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Dialog Edit Product 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Dialog Create Key 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Dialog Key Reveal 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Dialog Revoke Key 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Dialog Archive Product 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Dialog Add Provider Midtrans 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Dialog Add Provider Xendit 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Dialog Add Provider IDRX 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Dialog Edit Provider 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Provider Test Success 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Provider Test Failure 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Dialog Delete Provider 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Dialog Provider In Use 1366` (dialog or sheet over the page it belongs to)
- `Dashboard Dialog Live Mode 1366` (dialog or sheet over the page it belongs to)

### Dashboard: empty, error and loading states, desktop 1366 and phone 390

- `Dashboard Payments Empty 1366` (shared state component)
- `Dashboard Products Empty 1366` (shared state component)
- `Dashboard Overview Empty 1366` (shared state component)
- `Dashboard Table Empty Filter 1366` (shared state component)
- `Dashboard Error Banner 1366` (shared state component)
- `Dashboard Session Expired 1366` (shared state component)
- `Dashboard Skeleton 1366` (shared state component)
- `Dashboard Not Found 1366` (shared state component)
- `Dashboard Toasts 1366` (shared state component)
- `Dashboard Payments Empty 390` (shared state component)
- `Dashboard Products Empty 390` (shared state component)
- `Dashboard Skeleton 390` (shared state component)

### Dashboard: laptop 1024 (lg) and tablet 768 (md)

- `Dashboard Overview 1024` (`/`)
- `Dashboard Overview 768` (`/`)
- `Dashboard Payments 768` (`/payments`)
- `Dashboard Payment Detail 768` (`/payments/[id]`)

### Dashboard: phone 390 pages

- `Dashboard Login 390` (`/login`)
- `Dashboard Overview 390` (`/`)
- `Dashboard Payments 390` (`/payments`)
- `Dashboard Payment Detail 390` (`/payments/[id]`)
- `Dashboard Cash-ins 390` (`/cash-ins`)
- `Dashboard Products 390` (`/products`)
- `Dashboard Providers 390` (`/providers`)
- `Dashboard Webhooks 390` (`/webhooks`)
- `Dashboard Webhooks Outbound 390`
- `Dashboard Playground 390` (`/playground`)
- `Dashboard More 390`

### Dashboard: phone 390 sheets and full-screen forms

- `Dashboard Sheet New Product 390` (dialog or sheet over the page it belongs to)
- `Dashboard Sheet Create Key 390` (dialog or sheet over the page it belongs to)
- `Dashboard Sheet Key Reveal 390` (dialog or sheet over the page it belongs to)
- `Dashboard Sheet Mode Live 390` (dialog or sheet over the page it belongs to)
- `Dashboard Sheet Payments Filter 390` (dialog or sheet over the page it belongs to)
- `Dashboard Sheet Date Range 390` (dialog or sheet over the page it belongs to)
- `Dashboard Add Provider IDRX 390`
- `Dashboard Edit Provider 390`

### Dashboard: smallest phone 320 (XS)

- `Dashboard Overview 320` (`/`)
- `Dashboard Payments 320` (`/payments`)
- `Dashboard Payment Detail 320` (`/payments/[id]`)
- `Dashboard Products 320` (`/products`)
- `Dashboard Add Provider IDRX 320`
- `Dashboard Sheet New Product 320` (dialog or sheet over the page it belongs to)

## 6. Decisions the design made that differ from the first web implementation

- Products page shows keys inline per product (table inside the product card) instead of in a key dialog; Archive and Rotate secret sit in the row's overflow menu.
- Login error copy `That token was not accepted` and the webhook URL validation copy `Webhook URL must start with https://` are both implemented. The login screen shows no token format, env variable or server hint, and a 429 shows a retry countdown. The Wonder login artboards still carry the older copy.
- Provider forms: fee schedule editor rows are Method select + fixed Rp + percent + remove, with the default `*` row first; method labels in fee rows use short names (`BCA VA`). IDRX locks Mode to Live and warns that no sandbox exists.
- Outbound deliveries: Retry is enabled for pending and failed rows, disabled for delivered (matches the code; the brief had said failed only).
- Playground: advanced options (expires_in, return_url, IDRX wallet and chain) are a disclosure, stacked in one column; Create payment is at the end of the form card on desktop and a sticky bar on phones.
- Webhook JSON bodies must render one element per line (the design tool collapsed newlines); use a `<pre>` with `font-mono`.
- Phone bottom bars (tab bar, sticky action bar) are drawn at the end of the artboard; implement them sticky to the viewport bottom with safe-area padding.
- Dialog artboard heights are natural content heights, not fixed.
- The `Method Error` Snap artboard dims non-active rows to 3.3:1 on purpose (disabled while a charge is in flight).

## 7. Final decisions where the web app and the artboards differ

Where the code and an artboard disagree, this section is the source of truth. The Wonder file was last edited before these decisions.

### Shell and navigation

- The top bar owns the page title. `PageHeader` publishes title, subtitle and actions to `shell` state and renders only an sr-only `h1[data-testid=page-title]`. Actions sit in the top bar from md and in the content below md.
- Phones: pages reached from More (Products, Webhooks, Playground) show a back button that reopens More instead of the brand tile. `PageHeader` takes `phoneTitle` (Providers, Playground), `phoneHeading` and `phoneDescription` for the intro line that the top bar hides on phones. The document title keeps the full title.
- Toasts are bottom centre on desktop and above the tab bar on phones. The artboard `Dashboard Toasts 1366` shows top right; the code does not follow it.
- Dark mode is derived from the tokens, not designed.

### Data and loading

- Every fetch has a skeleton. Lists wait for the catalog (`meta`) before rendering rows, so provider and method slugs never flash. Filter selects, status chip counts, cash-in totals and the Playground form show placeholders while their data loads.
- A request error shows `Could not load <subject>` with `The server returned <status>.` for unstructured HTTP failures; a structured API error shows its own message. Actions that fail keep the `Request failed` title.
- The Overview shows `N paid of M closed` instead of a conversion delta, and hides the change line when current and previous periods are both zero. The API has no previous-period conversion.
- Not supported by the API, so not built: Change method, Try another method after a terminal state, the IDRX destination-wallet card, a customer-chose-method activity entry, Copy and Roll key per key (full keys are shown once), per-product payment count and last delivery, and a docs link on the Payments empty state.

### Per screen

- Overview: Recent payments sit inside one card (`DataList` `bare`). The chart shows three date labels on phones and a dashed baseline with the first and last date when empty. The "Days follow Asia/Jakarta" note is hidden on phones. Stat figures stay 24/30; 2 columns at md and lg, 4 from xl.
- Payments and Cash-ins: the pager is the footer of the list card, also when a filter matches nothing (`Page 1 · 0 shown`). Previous and Next have no icons, and a disabled one is a flat grey fill. The Created cell is two lines below lg, the row chevron shows from md, and phone cards have no provider mark and wrap their text. With no payments at all the filters and chips are hidden and the empty card fills the page height. Cash-in fee reads `− 1.050` in the table.
- Payment detail: below xl one column in the order summary, ledger, activity, then instructions, Snap link, simulator and metadata. From xl two columns.
- Webhooks: underline tabs from md, a segmented control with short labels (Inbound, Outbound) on phones; the accessible names stay `Inbound events` and `Outbound deliveries`. Inbound shows Verified, Payment and Error as separate columns from lg (merged under Outcome at md). Outbound shows Attempts and Next attempt as separate columns from lg. Phone filters are compact pills.
- Products: below lg a card is one column in this order: avatar and name, default provider, webhook URL, created date, the API keys box, then Edit, API keys and the overflow menu. The API keys button toggles the same box. At 320 the keys are always listed and the API keys button and the disclosure toggle are gone; the intro line is a count (`3 products · 4 API keys in sandbox`). The empty state is a centred narrow checklist; the header keeps its New product button.
- Providers: phone cards use a dark avatar, put the mode chip and the enabled switch on one row, box the credentials, and order the buttons Test then Edit (both outline).
- Playground: the first product is preselected. Mode is read-only and follows the top bar switch (the 390 artboard shows a selector). Checkout uses two radio cards.
- More: Data mode is a full-width rounded segmented control; Theme is a select row. It shows no token tail, host or version.
- Date range: selected days form one continuous band with rounded ends; the popover scrolls inside short viewports.
- IDRX has no sandbox, so its mode is locked to Live everywhere.

### Implementation notes

- shadcn files copied from the base-ui flavour used `data-checked`, `data-unchecked`, `data-active`, `data-open` and `data-closed`; bits-ui sets only `data-state`. Switch, Tabs, Dialog, Alert dialog, Sheet, Select, Tooltip and the sidebar menu button now use `data-[state=…]` (Tooltip matches `data-[state*=open]` because bits-ui reports `delayed-open` and `instant-open`). Measured in the browser: dialog, sheet and select run `enter` on open and `exit` on close and unmount afterwards, the dialog stays centred while it animates, and with reduced motion they still open and close. Alert dialog and Tooltip are not used by any page, so their animation is unmeasured.
- Rate limits answer `429 rate_limited` with `Retry-After`; see `docs/api.md`.
- Pending Wonder edits (blocked by the workspace layer limit until 2026-12-02 or a plan upgrade): restore the Foundations Controls badge row and pills, the pills on Webhooks Outbound 390, Products 1366 and Products Empty 1366, and apply the login copy.
