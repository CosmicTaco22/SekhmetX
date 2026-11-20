# Code signing and the installer

## Why sign

Unsigned programs make Windows SmartScreen say "Windows protected your PC" and
some antivirus tools flag them (Trend Micro did with an early build). On a Mac,
Gatekeeper refuses to open an unsigned app until it is allowed in System
Settings. Signing proves the program comes from you and has not been changed.

The release workflow (`.github/workflows/release.yml`) already contains the
signing steps. They run only when the certificates below are stored as
repository secrets; until then every build is published unsigned, as before.

## Windows

1. Get a code-signing certificate. Options, cheapest first:
   - **Microsoft Trusted Signing** (Azure, about US$10 a month; needs an
     identity check). It signs through Azure rather than with a `.pfx` file, so
     the workflow step would need changing to its own GitHub action.
   - An **OV code-signing certificate** from a certificate authority (Sectigo,
     DigiCert, SSL.com…), about US$200–400 a year. Recent ones come on a
     hardware token or a cloud service; ask for one you can export as `.pfx`, or
     for their GitHub Actions instructions.
   - An **EV certificate** removes the SmartScreen warning at once (OV builds
     reputation over the first downloads), but costs more and is always on a
     token.
2. With a `.pfx` file: on GitHub, open the repository → **Settings** →
   **Secrets and variables** → **Actions** → **New repository secret**, and add
   - `WINDOWS_CERT_PFX_BASE64`: the `.pfx` file as base64 (on Windows,
     PowerShell: `[Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx"))`),
   - `WINDOWS_CERT_PASSWORD`: its password.
3. The next tagged release signs `sekhmetx.exe` (before it is zipped) and the
   installer.

## macOS

1. Join the **Apple Developer Program** (US$99 a year) and create a
   **Developer ID Application** certificate; export it from Keychain Access as a
   `.p12` file with a password.
2. Create an **app-specific password** for your Apple ID (appleid.apple.com →
   Sign-In and Security).
3. Add these repository secrets:
   - `APPLE_CERT_P12_BASE64`: the `.p12` file as base64 (`base64 -i cert.p12`),
   - `APPLE_CERT_PASSWORD`: its password,
   - `APPLE_SIGNING_IDENTITY`: the certificate's name, e.g.
     `Developer ID Application: Your Name (TEAMID1234)`,
   - `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD`: for notarization (Apple
     checks the app and "staples" its approval to it).
4. The next tagged release signs and notarizes `SekhmetX.app`.

## The Windows installer

`packaging/windows/sekhmetx.iss` (Inno Setup 6) builds
`sekhmetx-vX.Y.Z-windows-x86_64-setup.exe` (and `-arm64-setup.exe`) beside the
`.zip` downloads. It installs for the current user only, without administrator
rights, into `%LOCALAPPDATA%\Programs\SekhmetX`, adds a Start-menu entry and
optionally a desktop icon, shows the licence, and has an uninstaller; English or
Spanish. The library and settings (`%LOCALAPPDATA%\SekhmetX`) are separate and are
kept when uninstalling. The in-app updater keeps working because the program
folder belongs to the user. CI builds the installer on every push to check it.

The `.zip` downloads stay, for people who prefer no installer (and for portable
use with a `portable.txt` file beside the program).
