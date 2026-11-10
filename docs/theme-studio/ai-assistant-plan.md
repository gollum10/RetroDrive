# Theme Studio AI: design help and AI-made pictures (plan, for owner approval)

Status: **approved by the owner 2026-11-10** (ADR-118); see section 6. Not built yet. This is the first RetroDrive feature that can send anything off the user's PC, so each build step adds its safety review section first.

Owner's words (2026-11-10): "It would be nice if the studio offered the ability to connect to a Frontier model or their own Private AI to assist with design." "I envision that the AI will be simple as signing into their account." "The other thing I would like it to do is add AI Generated pics."

## 1. What the user gets

1. **Ask for help with the design.** A small **Ask AI** box in the editor: "make this look like a neon arcade", "suggest a colour scheme from my background picture", "line up the game details on the right", "write a fun title". The answer shows as **suggested changes on the canvas**, with a one-line explanation. **Keep** applies them as one step that Undo reverses; **Discard** drops them. Nothing changes without a click.
2. **Make a picture with AI.** In **Add picture**, a second tab: **Make one with AI**. The user describes the picture ("pixel-art sunset over a city, wide"), picks a shape (wide background, square, tall), gets two to four choices, and picks one. It goes in like any other picture: placed on the screen, as the background, or as a frame. **Made with an AI tool** is ticked automatically, and the description is kept in the picture's credit.
3. **Works the way they want to pay or not pay.**
   - **Claude** (Anthropic): best design help. Uses the user's own Claude Console account.
   - **A private AI on this PC or network** (Ollama, LM Studio, or any OpenAI-compatible server): free, offline, nothing leaves the network; quality depends on the model they run.
   - **Pictures:** the user's own image-service account (OpenAI image models first; others later) or a private picture server on their PC (ComfyUI or Automatic1111/Forge). Claude doesn't make pictures.

## 2. "As simple as signing in": what's possible

Checked against Anthropic's authentication docs (2026-11-10). The Claude API accepts **API keys**, **Workload Identity Federation** (servers and CI) and **App Attest** (iOS and macOS apps only; usage is billed to the app maker). There is **no "Sign in with Claude" for third-party Windows apps**, and a Claude.ai Pro or Max subscription doesn't cover API use in other apps. A RetroDrive-run server holding one shared key would make the free app pay for everyone's use, and needs a server and accounts, which RetroDrive doesn't have.

So the closest honest equivalent is a **guided connect** that feels like signing in, done once:

1. **Connect Claude** opens the Claude Console in the browser. The user signs in or creates an account, and adds a payment method (pay as you go; Anthropic's own pricing).
2. RetroDrive opens the **Create key** page directly. The user clicks **Create key** and **Copy**.
3. Back in RetroDrive, **Paste**. RetroDrive checks the key with one tiny request, shows "Connected as <workspace>", and stores the key in **Windows Credential Manager**. It is never put in a theme, a file, a log or the catalog.
4. From then on it's just "Connected". **Disconnect** removes the key from Credential Manager.

The same flow, with its own pages, for an image service. A private server needs only its address (for example `http://localhost:11434`), found automatically when it's on the usual port.

If Anthropic adds a third-party sign-in for desktop apps later, it can replace steps 1 to 3 without changing anything else.

## 3. What is sent, and when

- **Nothing is sent until the user connects a service and presses Ask or Make.** No background calls, no telemetry, no analytics.
- **Sent for design help:** the current screen's layers as data (type, position, size, colours, text, which game detail each text shows), the screen's size, and the request typed. **Optionally**, ticked per request, a picture of the screen as drawn in the editor (the same drawing used for baking).
- **Never sent:** game names from the library, file or folder paths, drive names, installation details, the user's name, RetroDrive settings, other themes, or picture files unless the user ticks "include a picture of the screen".
- **Sent for pictures:** only the description typed and the size wanted.
- Each Ask box shows **what will be sent** in one line, with **Show exactly** revealing the full text and the picture.
- Results come back as data RetroDrive checks (below). Model text is never run as code, never used as a file path, and never opens links.
- What the service does with requests is governed by that service's terms; the connect screen links to them. A private server keeps everything on the user's machine or network.

## 4. How it works (for the build)

- **Calls are made by the Rust backend**, not the web view, so keys never reach page code. Claude: the Messages API over HTTPS (`POST https://api.anthropic.com/v1/messages`) with raw HTTP, since there's no official Rust SDK; default model **`claude-opus-5-5`** (the user can pick another Claude model in settings), adaptive thinking, streaming. Private servers: the OpenAI-compatible chat API at the address the user gave. Pictures: the image service's own API, or the local server's API.
- **One new HTTP library** (a small, widely used Rust client with rustls; MIT/Apache license, added to the license gate and notices). **No new library for keys:** Windows Credential Manager through the `windows-sys` crate RetroDrive already uses (one extra feature flag).
- **Fixed destinations only:** Anthropic's API host, the chosen image service's host, and the private address the user typed (local or private-network addresses only, never public ones for "private"). The web view's security policy is unchanged; it never talks to the internet.
- **Suggestions as checked data.** Claude is asked for a structured result (`output_config.format` with a JSON schema): a short explanation plus a list of edits from a small fixed set (add, move, resize, set colour, set text, set font size, link text to a game detail, reorder, hide). RetroDrive validates every edit against the current theme (layer exists, values in range, colours valid) and drops anything that doesn't fit, saying so. Applying uses the editor's existing edit functions, as one undoable step. Private models get the same schema; if their answer doesn't parse, the user sees "That model's answer couldn't be used" and nothing changes.
- **AI pictures** come back as image bytes, then go through the existing picture checks (`encodePictureFile`, the PNG normaliser, the picture store), exactly like a picture the user adds. Size limits as today.
- **Cost and limits shown.** Each answer shows roughly what it cost when the service reports usage (Claude does). A per-day request limit (default 50) protects against surprises; refusals and errors are explained plainly. Requests can be cancelled.
- **Skill levels:** Simple shows the Ask box and Make a picture; Standard and Pro add model choice and "Show exactly".

## 5. Build order (one PR each, after approval)

1. **Connect and disconnect** (Claude and a private server): the guided connect, key check, Credential Manager, settings page. Safety review section first (docs/18).
2. **Ask AI for design suggestions:** structured edits, preview on the canvas, Keep/Discard, what-is-sent line.
3. **Make a picture with AI:** image service and local picture server connect, the Make tab, choices, import with AI credit.
4. **Polish:** suggestions from a picture ("colours from my background"), cost display, per-day limit settings.

## 6. Owner decisions (2026-11-10)

Answered: (1) **Guided Connect** with the user's own key; (2) **OpenAI image models plus a private picture server** first; (3) default model **Claude Opus 5.5** (`claude-opus-5-5`), a cheaper model selectable in settings; (4) **add one HTTP library**. The questions as asked:


1. Approve the **guided connect with the user's own key** (section 2) as the way to "sign in"?
2. First picture service: **OpenAI image models** plus a **private ComfyUI or Automatic1111/Forge** server? Others (Stability AI, Google) can follow.
3. Default Claude model **`claude-opus-5-5`** (best results, about $4 per million words in and $20 per million out; a typical suggestion is a few cents), with a cheaper choice in settings?
4. OK to add **one HTTP library** to the app for these calls?
