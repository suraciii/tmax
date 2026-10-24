//! Native protected channels. No CRT descriptor table, console allocation, or byte probes.
use super::{Descriptor, Error, MAX_INPUT, Result, fs, input_error, read_line};
use std::{
    ffi::c_void,
    os::windows::io::{AsRawHandle, FromRawHandle},
    ptr,
};
use zeroize::Zeroizing;

type Handle = *mut c_void;
const FILE_TYPE_PIPE: u32 = 3;
const FILE_READ_DATA: u32 = 1;
const FILE_WRITE_DATA: u32 = 2;
const ENABLE_ECHO_INPUT: u32 = 4;
const ENABLE_LINE_INPUT: u32 = 2;
const ENABLE_PROCESSED_INPUT: u32 = 1;
const DUPLICATE_SAME_ACCESS: u32 = 2;

#[repr(C)]
struct IoStatus {
    status: usize,
    information: usize,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetStdHandle(kind: u32) -> Handle;
    fn GetCurrentProcess() -> Handle;
    fn GetFileType(handle: Handle) -> u32;
    fn DuplicateHandle(
        source_process: Handle,
        source: Handle,
        target_process: Handle,
        target: *mut Handle,
        access: u32,
        inherit: i32,
        options: u32,
    ) -> i32;
    fn CompareObjectHandles(first: Handle, second: Handle) -> i32;
    fn GetConsoleMode(handle: Handle, mode: *mut u32) -> i32;
    fn SetConsoleMode(handle: Handle, mode: u32) -> i32;
    fn ReadConsoleW(
        handle: Handle,
        buffer: *mut u16,
        count: u32,
        read: *mut u32,
        control: *mut c_void,
    ) -> i32;
    fn WriteConsoleW(
        handle: Handle,
        buffer: *const u16,
        count: u32,
        written: *mut u32,
        reserved: *mut c_void,
    ) -> i32;
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryInformationFile(
        handle: Handle,
        status: *mut IoStatus,
        information: *mut c_void,
        length: u32,
        class: u32,
    ) -> i32;
    fn NtQueryVolumeInformationFile(
        handle: Handle,
        status: *mut IoStatus,
        information: *mut c_void,
        length: u32,
        class: u32,
    ) -> i32;
}

fn handle(descriptor: Descriptor) -> Result<Handle> {
    let raw = match descriptor {
        // Existing grammar reserves 0/1/2 even when Windows assigns different values.
        0 => unsafe { GetStdHandle((-10i32) as u32) },
        1 => unsafe { GetStdHandle((-11i32) as u32) },
        2 => unsafe { GetStdHandle((-12i32) as u32) },
        value => value as Handle,
    };
    if raw.is_null() || raw as usize == usize::MAX {
        return Err(input_error());
    }
    Ok(raw)
}

fn is_pipe(handle: Handle) -> Result<bool> {
    match unsafe { GetFileType(handle) } {
        0 => Err(input_error()),
        FILE_TYPE_PIPE => {
            // GetFileType also reports sockets as pipes. Query the underlying
            // device, not GetNamedPipeInfo (which requires extra read-attribute
            // access on write-only handles). FileFsDeviceInformation=4,
            // FILE_DEVICE_NAMED_PIPE=0x11. No secret bytes are read or written.
            // https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntqueryvolumeinformationfile
            let mut device = [0u32; 2];
            let mut status = IoStatus {
                status: 0,
                information: 0,
            };
            if unsafe {
                NtQueryVolumeInformationFile(
                    handle,
                    &mut status,
                    device.as_mut_ptr().cast(),
                    std::mem::size_of_val(&device) as u32,
                    4,
                )
            } != 0
                || status.status as i32 != 0
                || status.information != std::mem::size_of_val(&device)
            {
                return Err(input_error());
            }
            Ok(device[0] == 0x11)
        }
        _ => Ok(false),
    }
}

fn query<T>(handle: Handle, information: &mut T, class: u32) -> Result<usize> {
    let mut status = IoStatus {
        status: 0,
        information: 0,
    };
    // FileAccessInformation=8 and FileInternalInformation=6 work for pipe file objects;
    // GetFileInformationByHandleEx explicitly excludes pipes. Require synchronous
    // STATUS_SUCCESS, not STATUS_PENDING or a partial/unsupported metadata result.
    // https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntqueryinformationfile
    if unsafe {
        NtQueryInformationFile(
            handle,
            &mut status,
            (information as *mut T).cast(),
            std::mem::size_of::<T>() as u32,
            class,
        )
    } != 0
        || status.status as i32 != 0
    {
        return Err(input_error());
    }
    Ok(status.information)
}

pub(super) fn qualify_pipe(descriptor: Descriptor, reading: bool) -> Result<()> {
    let raw = handle(descriptor)?;
    if !is_pipe(raw)? {
        return Err(input_error());
    }
    let mut access = 0u32;
    if query(raw, &mut access, 8)? != std::mem::size_of::<u32>()
        || access
            & (if reading {
                FILE_READ_DATA
            } else {
                FILE_WRITE_DATA
            })
            == 0
    {
        return Err(input_error());
    }
    // Also establish a usable identity before accepting a protected channel.
    pipe_id(raw)?;
    Ok(())
}

// Compare only live handles on the named-pipe device. Do not persist these IDs:
// the file system may reuse them after handles close. FileNameInformation can
// return an empty name for valid anonymous pipes on Windows.
// https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_internal_information
fn pipe_id(raw: Handle) -> Result<u64> {
    let mut id = 0u64;
    if query(raw, &mut id, 6)? != std::mem::size_of::<u64>() || id == 0 {
        return Err(input_error());
    }
    Ok(id)
}

pub(super) fn descriptors_alias(first: Descriptor, second: Descriptor) -> Result<bool> {
    let first = handle(first)?;
    let second = match handle(second) {
        Ok(raw) => raw,
        // Detached processes may have no stdout/stderr. An absent standard
        // handle cannot receive the protected output and is not an alias.
        Err(_) if matches!(second, 1 | 2) => return Ok(false),
        Err(error) => return Err(error),
    };
    if first == second || unsafe { CompareObjectHandles(first, second) } != 0 {
        return Ok(true);
    }
    if !is_pipe(first)? || !is_pipe(second)? {
        return Ok(false);
    }
    // Opposite ends are distinct file objects but share the pipe's internal ID.
    // This also covers unnamed anonymous pipes and duplicated handles without
    // consuming bytes or conflating unrelated pipes with empty names.
    Ok(pipe_id(first)? == pipe_id(second)?)
}

pub(super) fn duplicate_fd(descriptor: Descriptor) -> Result<fs::File> {
    let raw = handle(descriptor)?;
    let mut duplicate = ptr::null_mut();
    let process = unsafe { GetCurrentProcess() };
    if unsafe {
        DuplicateHandle(
            process,
            raw,
            process,
            &mut duplicate,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    } == 0
    {
        return Err(input_error());
    }
    // DuplicateHandle owns this new, explicitly noninheritable handle.
    Ok(unsafe { fs::File::from_raw_handle(duplicate) })
}

pub(super) fn read_descriptor(descriptor: Descriptor, permit_terminal: bool) -> Result<Vec<u8>> {
    let mut mode = 0;
    if permit_terminal && unsafe { GetConsoleMode(handle(descriptor)?, &mut mode) } != 0 {
        return read_hidden(&duplicate_fd(descriptor)?);
    }
    qualify_pipe(descriptor, true)?;
    read_line(&mut duplicate_fd(descriptor)?)
}

fn console(name: &str) -> Result<fs::File> {
    // Opening the attached console devices does not allocate or attach a console.
    // Never use redirected JSON stdin/stdout to prompt or reveal credentials.
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(name)
        .map_err(|_| {
            Error::new(
                "interactive_required",
                "No supported attached console is available",
            )
        })?;
    let mut mode = 0;
    if unsafe { GetConsoleMode(file.as_raw_handle(), &mut mode) } == 0 {
        return Err(Error::new(
            "interactive_required",
            "No supported attached console is available",
        ));
    }
    Ok(file)
}

pub(super) fn open_terminal() -> Result<fs::File> {
    console("CONOUT$")
}

pub(super) fn native_console_stdio() -> Result<(fs::File, fs::File)> {
    Ok((console("CONIN$")?, open_terminal()?))
}

pub(super) fn write_console_output(file: &mut fs::File, value: &str) -> Result<()> {
    write_console(file, value)
}

struct RestoreMode {
    handle: Handle,
    original: u32,
    restored: bool,
}
impl RestoreMode {
    fn hidden(file: &fs::File) -> Result<Self> {
        let handle = file.as_raw_handle();
        let mut original = 0;
        if unsafe { GetConsoleMode(handle, &mut original) } == 0
            || unsafe {
                // Raw input makes Ctrl-C a character, not a terminating signal:
                // cancellation restores mode even with JSON stdout redirected.
                SetConsoleMode(
                    handle,
                    original & !(ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT),
                )
            } == 0
        {
            return Err(input_error());
        }
        Ok(Self {
            handle,
            original,
            restored: false,
        })
    }

    fn restore(&mut self) -> Result<()> {
        if self.restored {
            return Ok(());
        }
        if unsafe { SetConsoleMode(self.handle, self.original) } == 0 {
            return Err(input_error());
        }
        self.restored = true;
        Ok(())
    }
}
impl Drop for RestoreMode {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn read_hidden(file: &fs::File) -> Result<Vec<u8>> {
    let mut restore = RestoreMode::hidden(file)?;
    let result = read_console_line(restore.handle);
    // Report restoration failure too, while Drop retries it on every error path.
    restore.restore()?;
    drop(restore);
    result
}

fn read_console_line(raw: Handle) -> Result<Vec<u8>> {
    let mut wide = Zeroizing::new(Vec::new());
    loop {
        let mut buffer = Zeroizing::new([0u16; 256]);
        let mut count = 0;
        if unsafe {
            ReadConsoleW(
                raw,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut count,
                ptr::null_mut(),
            )
        } == 0
            || count == 0
            || count as usize > buffer.len()
        {
            return Err(input_error());
        }
        for &unit in &buffer[..count as usize] {
            if edit_console_unit(&mut wide, unit)? {
                return decode_console_line(&wide);
            }
        }
    }
}

fn edit_console_unit(wide: &mut Vec<u16>, unit: u16) -> Result<bool> {
    match unit {
        3 | 26 => Err(Error::new(
            "cancelled",
            "Protected console input was cancelled",
        )),
        10 | 13 => Ok(true),
        8 | 127 => {
            if wide
                .pop()
                .is_some_and(|last| (0xdc00..=0xdfff).contains(&last))
                && wide
                    .last()
                    .is_some_and(|last| (0xd800..=0xdbff).contains(last))
            {
                wide.pop();
            }
            Ok(false)
        }
        _ if wide.len() >= MAX_INPUT => Err(input_error()),
        _ => {
            wide.push(unit);
            Ok(false)
        }
    }
}

fn decode_console_line(wide: &[u16]) -> Result<Vec<u8>> {
    let mut text = Zeroizing::new(String::from_utf16(wide).map_err(|_| input_error())?);
    if text.is_empty()
        || text.len() > MAX_INPUT
        || text.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n'))
    {
        return Err(input_error());
    }
    Ok(std::mem::take(&mut *text).into_bytes())
}

fn write_console(file: &fs::File, text: &str) -> Result<()> {
    // Fixed bounded chunks avoid a second full credential allocation and never
    // split a surrogate pair between WriteConsoleW calls.
    let mut buffer = Zeroizing::new([0u16; 256]);
    let mut used = 0;
    for character in text.chars() {
        if used + character.len_utf16() > buffer.len() {
            write_wide(file.as_raw_handle(), &buffer[..used])?;
            used = 0;
        }
        used += character.encode_utf16(&mut buffer[used..]).len();
    }
    write_wide(file.as_raw_handle(), &buffer[..used])
}

fn write_wide(raw: Handle, mut units: &[u16]) -> Result<()> {
    while !units.is_empty() {
        let mut written = 0;
        if unsafe {
            WriteConsoleW(
                raw,
                units.as_ptr(),
                units.len() as u32,
                &mut written,
                ptr::null_mut(),
            )
        } == 0
            || written == 0
            || written as usize > units.len()
        {
            return Err(input_error());
        }
        units = &units[written as usize..];
    }
    Ok(())
}

pub(super) fn read_terminal(prompt: &str) -> Result<Vec<u8>> {
    let input = console("CONIN$")?;
    let output = open_terminal()?;
    write_console(&output, prompt)?;
    let result = read_hidden(&input);
    let _ = write_console(&output, "\r\n");
    result
}

pub(super) fn deliver_terminal(file: &fs::File, bytes: &[u8]) -> Result<()> {
    let text = std::str::from_utf8(bytes).map_err(|_| super::unavailable())?;
    write_console(file, text)
        .and_then(|_| write_console(file, "\r\n"))
        .map_err(|_| {
            Error::new(
                "secret_delivery_failed",
                "Protected secret delivery failed; delivery may have been partial",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetHandleInformation(handle: Handle, flags: *mut u32) -> i32;
        fn SetStdHandle(kind: u32, handle: Handle) -> i32;
        fn CreateNamedPipeW(
            name: *const u16,
            open_mode: u32,
            pipe_mode: u32,
            max_instances: u32,
            out_buffer: u32,
            in_buffer: u32,
            timeout: u32,
            security: *mut c_void,
        ) -> Handle;
    }

    fn descriptor(value: &impl AsRawHandle) -> Descriptor {
        value.as_raw_handle() as usize
    }

    #[test]
    fn pipe_direction_identity_and_noninheritance_are_qualified() {
        let (reader, writer) = std::io::pipe().unwrap();
        let (other_reader, other_writer) = std::io::pipe().unwrap();
        let read = descriptor(&reader);
        let write = descriptor(&writer);
        qualify_pipe(read, true).unwrap();
        qualify_pipe(write, false).unwrap();
        assert!(qualify_pipe(read, false).is_err());
        assert!(qualify_pipe(write, true).is_err());
        assert!(descriptors_alias(read, write).unwrap());
        assert!(!descriptors_alias(read, descriptor(&other_reader)).unwrap());
        assert!(!descriptors_alias(write, descriptor(&other_writer)).unwrap());
        let duplicate = duplicate_fd(read).unwrap();
        assert!(descriptors_alias(read, descriptor(&duplicate)).unwrap());
        assert!(descriptors_alias(write, descriptor(&duplicate)).unwrap());
        let mut flags = u32::MAX;
        assert_ne!(
            unsafe { GetHandleInformation(duplicate.as_raw_handle(), &mut flags) },
            0
        );
        assert_eq!(flags & 1, 0, "protected duplicates must not be inherited");
        let file = tempfile::tempfile().unwrap();
        assert!(qualify_pipe(descriptor(&file), true).is_err());
        assert!(qualify_pipe(descriptor(&file), false).is_err());
        assert!(qualify_pipe(usize::MAX, true).is_err());
        assert!(qualify_pipe(usize::MAX - 1, false).is_err());
    }

    #[test]
    fn protected_pipe_round_trip_uses_native_handles() {
        let (reader, mut writer) = std::io::pipe().unwrap();
        writer.write_all("密钥-🔑\r\n".as_bytes()).unwrap();
        drop(writer);
        assert_eq!(
            read_descriptor(descriptor(&reader), false).unwrap(),
            "密钥-🔑".as_bytes()
        );

        let (mut reader, writer) = std::io::pipe().unwrap();
        let mut request = super::super::tests::request();
        request
            .arguments
            .insert("non-interactive".into(), vec!["true".into()]);
        request
            .arguments
            .insert("output-fd".into(), vec![descriptor(&writer).to_string()]);
        let mut output = super::super::qualify_output(&request).unwrap();
        assert_eq!(output.descriptor(), Some(descriptor(&writer)));
        output.deliver("密钥-🔑".as_bytes()).unwrap();
        drop(output);
        drop(writer);
        let mut received = Vec::new();
        reader.read_to_end(&mut received).unwrap();
        assert_eq!(received, "密钥-🔑\n".as_bytes());
    }

    #[test]
    fn named_pipe_ends_share_identity_without_aliasing_anonymous_pipes() {
        let name = format!(r"\\.\pipe\tmax-secret-test-{}", uuid::Uuid::new_v4());
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // Outbound, synchronous byte pipe. Opening the read end connects it.
        let raw =
            unsafe { CreateNamedPipeW(wide.as_ptr(), 2, 0, 1, 1024, 1024, 0, ptr::null_mut()) };
        assert_ne!(raw as usize, usize::MAX);
        let mut writer = unsafe { fs::File::from_raw_handle(raw) };
        let reader = fs::File::open(&name).unwrap();
        qualify_pipe(descriptor(&reader), true).unwrap();
        qualify_pipe(descriptor(&writer), false).unwrap();
        assert!(descriptors_alias(descriptor(&reader), descriptor(&writer)).unwrap());
        let (other, _) = std::io::pipe().unwrap();
        assert!(!descriptors_alias(descriptor(&reader), descriptor(&other)).unwrap());
        writer.write_all(b"synthetic-named-key\n").unwrap();
        assert_eq!(
            read_descriptor(descriptor(&reader), false).unwrap(),
            b"synthetic-named-key"
        );
    }

    #[test]
    fn console_editor_handles_enter_cancel_backspace_and_unicode() {
        let mut wide = Vec::new();
        for unit in "密钥🔑".encode_utf16() {
            assert!(!edit_console_unit(&mut wide, unit).unwrap());
        }
        assert!(!edit_console_unit(&mut wide, 8).unwrap());
        assert_eq!(decode_console_line(&wide).unwrap(), "密钥".as_bytes());
        assert!(!edit_console_unit(&mut wide, 127).unwrap());
        assert_eq!(decode_console_line(&wide).unwrap(), "密".as_bytes());
        assert!(edit_console_unit(&mut wide, 13).unwrap());
        assert!(edit_console_unit(&mut wide, 10).unwrap());
        assert_eq!(
            edit_console_unit(&mut wide, 3).unwrap_err().code,
            "cancelled"
        );
        assert_eq!(
            edit_console_unit(&mut wide, 26).unwrap_err().code,
            "cancelled"
        );
        assert!(decode_console_line(&[]).is_err());
        assert!(decode_console_line(&[0xd800]).is_err());
        assert!(decode_console_line(&[0]).is_err());
        assert!(decode_console_line(&[b'a' as u16, 13, b'b' as u16]).is_err());
        assert!(decode_console_line(&vec![0x4e00; MAX_INPUT / 3 + 1]).is_err());
        assert!(edit_console_unit(&mut vec![b'a' as u16; MAX_INPUT], b'b' as u16).is_err());
    }

    // Run standard-handle mutation only in an isolated test process, never in
    // parallel with the test harness or other tests in the parent process.
    #[test]
    fn standard_input_mapping_and_output_aliases_are_enforced() {
        const CHILD: &str = "TMAX_TEST_PROTECTED_STANDARD_HANDLES";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "secrets::windows::tests::standard_input_mapping_and_output_aliases_are_enforced", "--nocapture"])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        struct RestoreStandard([Handle; 3]);
        impl Drop for RestoreStandard {
            fn drop(&mut self) {
                for (index, raw) in self.0.iter().enumerate() {
                    unsafe {
                        SetStdHandle((-10 - index as i32) as u32, *raw);
                    }
                }
            }
        }
        let _restore = RestoreStandard([
            unsafe { GetStdHandle((-10i32) as u32) },
            unsafe { GetStdHandle((-11i32) as u32) },
            unsafe { GetStdHandle((-12i32) as u32) },
        ]);
        let (reader, mut writer) = std::io::pipe().unwrap();
        assert_ne!(
            unsafe { SetStdHandle((-10i32) as u32, reader.as_raw_handle()) },
            0
        );
        writer.write_all("标准输入\n".as_bytes()).unwrap();
        assert_eq!(read_descriptor(0, true).unwrap(), "标准输入".as_bytes());
        for standard in [1, 2] {
            assert_ne!(
                unsafe { SetStdHandle((-10 - standard) as u32, writer.as_raw_handle()) },
                0
            );
            let duplicate = duplicate_fd(descriptor(&writer)).unwrap();
            let mut request = super::super::tests::request();
            request
                .arguments
                .insert("output-fd".into(), vec![descriptor(&duplicate).to_string()]);
            assert!(super::super::qualify_output(&request).is_err());
        }
        let file = tempfile::tempfile().unwrap();
        assert_ne!(
            unsafe { SetStdHandle((-10i32) as u32, file.as_raw_handle()) },
            0
        );
        assert!(read_descriptor(0, true).is_err());
    }

    #[test]
    #[ignore = "Requires an attached Windows console; does not create or attach one"]
    fn attached_console_mode_restores_after_success_error_and_cancel() {
        let input = console("CONIN$").unwrap();
        let mut original = 0;
        assert_ne!(
            unsafe { GetConsoleMode(input.as_raw_handle(), &mut original) },
            0
        );
        for outcome in [0, 1, 2] {
            let result = (|| -> Result<()> {
                let mut restore = RestoreMode::hidden(&input)?;
                let mut hidden = 0;
                assert_ne!(
                    unsafe { GetConsoleMode(input.as_raw_handle(), &mut hidden) },
                    0
                );
                assert_eq!(
                    hidden & (ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT),
                    0
                );
                match outcome {
                    0 => restore.restore(),
                    1 => Err(input_error()),
                    _ => edit_console_unit(&mut Vec::new(), 3).map(|_| ()),
                }
            })();
            assert_eq!(result.is_ok(), outcome == 0);
            if outcome == 2 {
                assert_eq!(result.unwrap_err().code, "cancelled");
            }
            let mut restored = 0;
            assert_ne!(
                unsafe { GetConsoleMode(input.as_raw_handle(), &mut restored) },
                0
            );
            assert_eq!(restored, original);
        }
    }
}
