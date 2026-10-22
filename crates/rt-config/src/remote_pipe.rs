//! Named pipes for [`super`] on Windows. Pipe names are shared by every user
//! of the machine, so each pipe lets only its own user connect, and a client
//! talks only to a pipe whose server runs as the same user.

use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GENERIC_READ,
    GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    EqualSid, GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY,
    TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeServerProcessId, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    WaitNamedPipeW,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

const BUFFER: u32 = 64 * 1024;

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

/// Closes a handle when dropped.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // Safety: the handle is open and owned here.
        unsafe { CloseHandle(self.0) };
    }
}

/// The user a process token belongs to, as the bytes of its SID.
fn token_user(token: HANDLE) -> io::Result<Vec<u8>> {
    let mut size = 0u32;
    // Safety: the first call only asks for the size.
    unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut size) };
    let mut buffer = vec![0u8; size as usize];
    // Safety: `buffer` has the `size` bytes the call asked for.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(buffer)
}

/// The SID in a [`token_user`] buffer.
fn sid_of(buffer: &[u8]) -> *mut c_void {
    // Safety: the buffer holds a TOKEN_USER, as GetTokenInformation wrote it.
    unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid }
}

fn process_user(process: HANDLE) -> io::Result<Vec<u8>> {
    let mut token: HANDLE = null_mut();
    // Safety: `token` receives a handle we then own.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = Owned(token);
    token_user(token.0)
}

fn current_user() -> io::Result<Vec<u8>> {
    // Safety: the current process's pseudo handle needs no closing.
    process_user(unsafe { GetCurrentProcess() })
}

/// A security descriptor that gives only `user` access: `D:P(A;;GA;;;<sid>)`.
struct Descriptor(PSECURITY_DESCRIPTOR);

impl Drop for Descriptor {
    fn drop(&mut self) {
        // Safety: allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW.
        unsafe { LocalFree(self.0) };
    }
}

fn only_user(user: &[u8]) -> io::Result<Descriptor> {
    let mut text: *mut u16 = null_mut();
    // Safety: `text` receives a string we free below.
    if unsafe { ConvertSidToStringSidW(sid_of(user), &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // Safety: a NUL-terminated string from ConvertSidToStringSidW.
    let sid = unsafe {
        let len = (0..).take_while(|&i| *text.add(i) != 0).count();
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        sid
    };
    let sddl: Vec<u16> = format!("D:P(A;;GA;;;{sid})")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    // Safety: `descriptor` receives memory owned by the returned Descriptor.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Descriptor(descriptor))
}

/// Connect to the pipe `name`, if its server runs as this user.
pub fn connect(name: &Path) -> io::Result<File> {
    let wide = wide(name);
    let handle = loop {
        // Safety: `wide` is a NUL-terminated path.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                null(),
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            break handle;
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_PIPE_BUSY as i32) {
            return Err(error);
        }
        // Every instance is busy: wait for one, briefly.
        // Safety: `wide` is a NUL-terminated path.
        if unsafe { WaitNamedPipeW(wide.as_ptr(), 2000) } == 0 {
            return Err(error);
        }
    };
    // Safety: a pipe handle we own from here on.
    let file = unsafe { File::from_raw_handle(handle as RawHandle) };
    let mut pid = 0u32;
    // Safety: `handle` is the open client end.
    if unsafe { GetNamedPipeServerProcessId(handle, &mut pid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // Safety: `server` is closed by Owned.
    let server = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if server.is_null() {
        return Err(io::Error::last_os_error());
    }
    let server = Owned(server);
    let (theirs, ours) = (process_user(server.0)?, current_user()?);
    // Safety: both are SIDs from TOKEN_USER buffers.
    if unsafe { EqualSid(sid_of(&theirs), sid_of(&ours)) } == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "another user's process holds this pipe",
        ));
    }
    Ok(file)
}

/// The server end: always one instance waiting, so a client never finds the
/// pipe missing between two connections.
pub struct Listener {
    name: Vec<u16>,
    descriptor: Descriptor,
    waiting: HANDLE,
}

// Safety: the handle is only used from the thread that owns the Listener.
unsafe impl Send for Listener {}

impl Drop for Listener {
    fn drop(&mut self) {
        // Safety: the waiting instance is open and owned here.
        unsafe { CloseHandle(self.waiting) };
    }
}

impl Listener {
    /// Create the pipe. Fails with `AddrInUse` if it exists already, as the
    /// first instance is created with `FILE_FLAG_FIRST_PIPE_INSTANCE`.
    pub fn bind(name: &Path) -> io::Result<Self> {
        let descriptor = only_user(&current_user()?)?;
        let name = wide(name);
        let waiting = create(&name, &descriptor, true).map_err(|e| {
            if e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) {
                io::Error::new(io::ErrorKind::AddrInUse, "another instance is running")
            } else {
                e
            }
        })?;
        Ok(Self {
            name,
            descriptor,
            waiting,
        })
    }

    /// Wait for a client, and return its connection.
    pub fn accept(&mut self) -> io::Result<File> {
        // Safety: `waiting` is an open instance; no OVERLAPPED, so this blocks.
        let connected = unsafe { ConnectNamedPipe(self.waiting, null_mut()) } != 0
            || io::Error::last_os_error().raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32);
        if !connected {
            return Err(io::Error::last_os_error());
        }
        let next = create(&self.name, &self.descriptor, false)?;
        let client = std::mem::replace(&mut self.waiting, next);
        // Safety: the connected instance, owned by the File from here on.
        Ok(unsafe { File::from_raw_handle(client as RawHandle) })
    }
}

fn create(name: &[u16], descriptor: &Descriptor, first: bool) -> io::Result<HANDLE> {
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    let flags = if first {
        PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE
    } else {
        PIPE_ACCESS_DUPLEX
    };
    // Safety: `name` is NUL-terminated and `attributes` outlives the call.
    let handle = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            flags,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            BUFFER,
            BUFFER,
            0,
            &attributes,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(handle)
}
