# Deployment & Release Guide

Vesper is distributed primarily as a standalone binary release tarball. The release process is **100% automated** requiring zero manual setup on your end.

## How to Release a New Version

You do not need to manually create tags or draft releases. Whenever you are ready to publish:

1. **Commit your changes:**
   ```bash
   git add .
   git commit -m "feat: added new search functionality"
   ```

2. **Push to the `main` branch:**
   ```bash
   git push origin main
   ```

**That's it!** 

## What Happens Automatically?

Once you push to `main`:
1. A GitHub Action catches the push and uses an **Auto Tag Assigner** to automatically bump the version number (e.g., from `v1.0.1` to `v1.0.2`) and push a new tag.
2. It generates a detailed changelog automatically based on your commit messages.
3. It compiles the Rust application in release mode, strips the binary, and packages it as a tarball (`vesper-linux-x86_64.tar.gz`) along with the `.desktop` file and scalable SVG icon.
4. It publishes a new official Release on your GitHub page with the generated changelog and attaches the release tarball.

## Commit Message Tips (Optional)
The auto-tagger uses standard prefixes to figure out how much to bump the version:
* `fix: ...` -> Bumps the **patch** version (e.g. 1.0.0 -> 1.0.1)
* `feat: ...` -> Bumps the **minor** version (e.g. 1.0.0 -> 1.1.0)
* *Any standard commit just bumps the patch version by default.*
