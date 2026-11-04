# 12 | VS Code, Windows and GitHub Development Guide

## Package content
Open this folder in VS Code. Existing source is under `src/` and `src-tauri/`; design docs are in `docs/`. No prebuilt installer is included. Existing native behavior needs verification on your Windows machine.

## Windows prerequisites
- Git and Visual Studio Code.
- Node.js LTS and npm.
- Rust via rustup with stable **MSVC** toolchain.
- Microsoft Visual Studio C++ Build Tools / Desktop development with C++.
- WebView2 runtime (typically available on supported Windows installs).
- Follow the maintained Tauri v2 prerequisites: https://v2.tauri.app/start/prerequisites/.

## Run
```powershell
npm install
npm run tauri dev
```
Frontend-only preview: `npm run dev` (native Tauri APIs not available in a plain browser). Build frontend: `npm run build`; Windows desktop distributable: `npm run tauri build` after local prerequisites are met.

## Suggested repository layout
```text
retrodrive-studio/
  docs/                 # product/design specifications
  src/                  # React UI starter
  src-tauri/            # Rust + native application
  .github/workflows/    # add CI once configured
  README.md
  package.json
```
Later split Rust modules by catalog, storage, jobs, platforms and deployment. Avoid premature multi-repository fragmentation.

## GitHub setup
Create an empty **private** repo `retrodrive-studio`. In the extracted directory:
```powershell
git init
git add .
git commit -m "Initial scaffold and product design"
git branch -M main
git remote add origin https://github.com/YOUR_USERNAME/retrodrive-studio.git
git push -u origin main
```
Replace the placeholder with your real GitHub user/org. Do not commit provider API keys or local database files.

## Branch and review conventions
`main` remains stable; branches `feature/RDS-###-name`, `fix/RDS-###-name`. Small pull requests, one acceptance criterion set per change, include screenshots for screen changes and tests for storage changes.

## Development discipline
- Never run an imaging workflow against a real system disk during development.
- Create fixture folders and sacrificial virtual/removable volumes for testing.
- Keep local appdata, caches and game collections outside repo; `.gitignore` any generated artifacts.
- Prefer typed Tauri commands returning structured errors and cancellable job IDs.
- Migrations required for DB schema changes; backup before incompatible transitions.
- Add mocked adapters before implementing disk writes.

## Troubleshooting build
If `npm run tauri dev` fails, record the **first** error plus output from `node -v`, `npm -v`, `rustc -V` and `cargo -V`. Check Rust MSVC prerequisites, installed C++ build tools, and WebView2. Do not assume all compiler errors are app defects.

## AI-assisted work
Use `14-AI-CODING-BRIEF.md` together with the specific roadmap issue. Request a small diff, automated tests and actual tool/run output; reject claims of having tested a Windows binary without Windows execution evidence.
