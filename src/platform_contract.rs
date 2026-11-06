//! The Windows and Unix halves of each platform twin (`restart`, `shell`, `ipc::pipe`, `local`,
//! `detached`, `https`) must agree on their signatures: shared code calls them without a `cfg`.
//! Each constant fails to compile on the platform whose twin drifts, so CI catches it on its own leg.
use crate::{
    detached::{Child, spawn},
    ipc::{Request, Response, pipe},
    local::{PipeServer, Scope},
};
use std::{ffi::OsStr, path::Path, process::ExitStatus, time::Duration};

type Done = Result<(), String>;

const _: fn(bool) -> Done = crate::restart::launch;
const _: fn(u32) -> Done = crate::restart::wait_for_parent;

const _: fn(&OsStr) -> Done = crate::shell::open;
const _: fn(&Path) -> Done = crate::shell::edit_configuration;

const _: fn(&Scope, &Request, Duration) -> Result<Response, String> = pipe::request;
const _: fn(&Scope, &Request, Duration) -> Result<Response, String> = pipe::request_if_running;

const _: fn(&str) -> Result<Scope, String> = Scope::current;
const _: fn(&Scope) -> Result<Option<crate::local::Ownership>, String> = Scope::acquire;
const _: fn(&Scope) -> Result<PipeServer, String> = Scope::create_pipe;

const _: fn(&Path, &[&OsStr], bool) -> Result<Child, String> = spawn;
const _: fn(&Child) -> u32 = Child::id;
const _: fn(&mut Child) -> Done = Child::kill;
const _: fn(&mut Child) -> Result<Option<ExitStatus>, String> = Child::try_wait;
const _: fn(&mut Child) -> Result<ExitStatus, String> = Child::wait;

type Http = Result<crate::https::Response, crate::https::HttpError>;
type RequestAt = fn(Option<&[u8]>, bool, &str, u16, &str, &str, Duration) -> Http;
const _: RequestAt = crate::https::request_at;
