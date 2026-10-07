# Dependency licenses

Run `cargo fetch --locked`, `npm ci`, then `python3 scripts/licenses.py`.
The generator copies original license / copyright notices and creates a
complete versioned inventory. CI requires license generation before packaging.
