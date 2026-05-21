# Simple HTTP File server in Rust

Side project of mine to further understand and practice with the http and async crates.

Using `hyper` as the base for my http server and obviously `tokio` for the async IO runtime.

Features :
- [x] Very simple CLI to handle parameters (obviously with `clap` derive)
- [x] File download
- [x] Mime guessing for correct response headers (with `mime_guess`)
- [x] File upload (with `multer` to parse multipart data)
- [x] Password (very simple auth "Authorization: <password>")
- [x] Directory listing (with `maud` as an html renderer)
- [x] File caching
- [ ] Directory listings caching
- [x] Path traversal filtering
- [ ] Opt-in file upload rather than enabled by default (could be dangerous)
- [ ] Parameter to set a limit for file upload
- [ ] Better caching algorithm than "let's cache everything and pray I don't run out of memory"
- [ ] Implement proper simple auth with username:password as b64 and all the other related things (I didn't document myself)
- [ ] Graceful shutdown
- [ ] Change logging system to `tracing`

Feel free to drop me a feedback on the codebase or about a cool feature I can implement in the [issues](https://github.com/Croos3r/http-file-server-rs/issues)
