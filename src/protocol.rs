use std::{
    io::{self, Read as _, Write as _},
    os::unix::{
        io::{AsRawFd as _, FromRawFd as _, OwnedFd, RawFd},
        net::UnixStream,
    },
};

pub const MAGIC: [u8; 4] = *b"TPRT";
pub const VERSION: u16 = 0;

pub const MSG_HELLO: u16 = 1;
pub const MSG_HELLO_ACK: u16 = 2;
pub const MSG_PRESENT: u16 = 3;
pub const MSG_PRESENT_ACK: u16 = 17;
pub const MSG_KEY: u16 = 4;
pub const MSG_SHUTDOWN: u16 = 5;
pub const MSG_MOUSE_MOVE: u16 = 6;
pub const MSG_MOUSE_BUTTON: u16 = 7;
pub const MSG_MOUSE_WHEEL: u16 = 8;
pub const MSG_MOUSE_ENTER: u16 = 9;
pub const MSG_MOUSE_LEAVE: u16 = 10;
pub const MSG_CLIPBOARD_SET: u16 = 11;
pub const MSG_SND: u16 = 12;
pub const MSG_MUTE: u16 = 13;
pub const MSG_PALETTE_COLOR_SET: u16 = 14;
pub const MSG_SETTINGS_PUSH: u16 = 15;
pub const MSG_SETTINGS_POP: u16 = 16;

pub const KEY_STATE_UP: u32 = 0;
pub const KEY_STATE_DOWN: u32 = 1;

pub const MOUSE_BUTTON_LEFT: u32 = 1;
pub const MOUSE_BUTTON_RIGHT: u32 = 2;
pub const MOUSE_BUTTON_MIDDLE: u32 = 3;
pub const MOUSE_BUTTON_BACK: u32 = 4;
pub const MOUSE_BUTTON_FORWARD: u32 = 5;
pub const MOUSE_BUTTON_OTHER_BASE: u32 = 0x8000;

pub const KEY_ESCAPE: u32 = 0x0100;
pub const KEY_ENTER: u32 = 0x0101;
pub const KEY_BACKSPACE: u32 = 0x0102;
pub const KEY_DELETE: u32 = 0x0103;
pub const KEY_TAB: u32 = 0x0104;
pub const KEY_HOME: u32 = 0x0105;
pub const KEY_END: u32 = 0x0106;
pub const KEY_PAGE_UP: u32 = 0x0107;
pub const KEY_PAGE_DOWN: u32 = 0x0108;
pub const KEY_INSERT: u32 = 0x0109;

pub const KEY_SHIFT: u32 = 0x0110;
pub const KEY_CONTROL: u32 = 0x0111;
pub const KEY_ALT: u32 = 0x0112;
pub const KEY_SUPER: u32 = 0x0113;

pub const KEY_F1: u32 = 0x0300;
pub const KEY_F2: u32 = 0x0301;
pub const KEY_F3: u32 = 0x0302;
pub const KEY_F4: u32 = 0x0303;
pub const KEY_F5: u32 = 0x0304;
pub const KEY_F6: u32 = 0x0305;
pub const KEY_F7: u32 = 0x0306;
pub const KEY_F8: u32 = 0x0307;
pub const KEY_F9: u32 = 0x0308;
pub const KEY_F10: u32 = 0x0309;
pub const KEY_F11: u32 = 0x030a;
pub const KEY_F12: u32 = 0x030b;
pub const KEY_LEFT: u32 = 0x0200;
pub const KEY_RIGHT: u32 = 0x0201;
pub const KEY_UP: u32 = 0x0202;
pub const KEY_DOWN: u32 = 0x0203;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Msg {
    pub kind: u16,
    pub a: u32,
    pub b: u32,
}

impl Msg {
    pub const LEN: usize = 16;

    pub fn hello() -> Self {
        Self {
            kind: MSG_HELLO,
            a: 0,
            b: 0,
        }
    }

    pub fn hello_ack(width: u32, height: u32) -> Self {
        Self {
            kind: MSG_HELLO_ACK,
            a: width,
            b: height,
        }
    }

    pub fn present(seq: u32) -> Self {
        Self {
            kind: MSG_PRESENT,
            a: seq,
            b: 0,
        }
    }

    pub fn present_ack(seq: u32) -> Self {
        Self {
            kind: MSG_PRESENT_ACK,
            a: seq,
            b: 0,
        }
    }

    pub fn key(code: u32, down: bool) -> Self {
        Self {
            kind: MSG_KEY,
            a: code,
            b: if down { KEY_STATE_DOWN } else { KEY_STATE_UP },
        }
    }

    pub fn mouse_move(x: u32, y: u32) -> Self {
        Self {
            kind: MSG_MOUSE_MOVE,
            a: x,
            b: y,
        }
    }

    pub fn mouse_button(button: u32, down: bool) -> Self {
        Self {
            kind: MSG_MOUSE_BUTTON,
            a: button,
            b: if down { KEY_STATE_DOWN } else { KEY_STATE_UP },
        }
    }

    pub fn mouse_wheel(dx: i32, dy: i32) -> Self {
        Self {
            kind: MSG_MOUSE_WHEEL,
            a: dx as u32,
            b: dy as u32,
        }
    }

    pub fn mouse_enter() -> Self {
        Self {
            kind: MSG_MOUSE_ENTER,
            a: 0,
            b: 0,
        }
    }

    pub fn mouse_leave() -> Self {
        Self {
            kind: MSG_MOUSE_LEAVE,
            a: 0,
            b: 0,
        }
    }

    pub fn clipboard_set(byte_len: u32) -> Self {
        Self {
            kind: MSG_CLIPBOARD_SET,
            a: byte_len,
            b: 0,
        }
    }

    pub fn snd(ona: u32) -> Self {
        Self {
            kind: MSG_SND,
            a: ona,
            b: 0,
        }
    }

    pub fn mute(val: bool) -> Self {
        Self {
            kind: MSG_MUTE,
            a: val as u32,
            b: 0,
        }
    }

    pub fn palette_color_set(color_index: u32, rgba: u32) -> Self {
        Self {
            kind: MSG_PALETTE_COLOR_SET,
            a: color_index,
            b: rgba,
        }
    }

    pub fn settings_push() -> Self {
        Self {
            kind: MSG_SETTINGS_PUSH,
            a: 0,
            b: 0,
        }
    }

    pub fn settings_pop() -> Self {
        Self {
            kind: MSG_SETTINGS_POP,
            a: 0,
            b: 0,
        }
    }

    pub fn shutdown() -> Self {
        Self {
            kind: MSG_SHUTDOWN,
            a: 0,
            b: 0,
        }
    }

    pub fn to_bytes(self) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];
        out[0..4].copy_from_slice(&MAGIC);
        out[4..6].copy_from_slice(&VERSION.to_le_bytes());
        out[6..8].copy_from_slice(&self.kind.to_le_bytes());
        out[8..12].copy_from_slice(&self.a.to_le_bytes());
        out[12..16].copy_from_slice(&self.b.to_le_bytes());
        out
    }

    pub fn from_bytes(buf: [u8; Self::LEN]) -> io::Result<Self> {
        if buf[0..4] != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "temple-rt: bad magic",
            ));
        }
        let version = u16::from_le_bytes([buf[4], buf[5]]);
        if version != VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("temple-rt: unsupported version {version}"),
            ));
        }
        let kind = u16::from_le_bytes([buf[6], buf[7]]);
        let a = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let b = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
        Ok(Self { kind, a, b })
    }
}

pub fn write_msg(stream: &mut UnixStream, msg: Msg) -> io::Result<()> {
    stream.write_all(&msg.to_bytes())
}

pub fn read_msg(stream: &mut UnixStream) -> io::Result<Msg> {
    let mut buf = [0u8; Msg::LEN];
    stream.read_exact(&mut buf)?;
    Msg::from_bytes(buf)
}

pub fn send_msg_with_fd(stream: &UnixStream, msg: Msg, fd: RawFd) -> io::Result<()> {
    use nix::sys::socket::{ControlMessage, MsgFlags, sendmsg};
    use std::io::IoSlice;

    let bytes = msg.to_bytes();
    let iov = [IoSlice::new(&bytes)];
    let cmsg = [ControlMessage::ScmRights(&[fd])];

    sendmsg::<()>(stream.as_raw_fd(), &iov, &cmsg, MsgFlags::empty(), None)
        .map(|_| ())
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
}

pub fn recv_msg_with_fd(stream: &mut UnixStream) -> io::Result<(Msg, Option<OwnedFd>)> {
    use nix::sys::socket::{ControlMessageOwned, MsgFlags, recvmsg};
    use std::io::IoSliceMut;

    let mut buf = [0u8; Msg::LEN];
    let mut iov = [IoSliceMut::new(&mut buf)];
    let mut cmsg_space = nix::cmsg_space!([RawFd; 1]);

    let msg = recvmsg::<()>(
        stream.as_raw_fd(),
        &mut iov,
        Some(&mut cmsg_space),
        MsgFlags::empty(),
    )
    .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

    let bytes_read = msg.bytes;
    if bytes_read == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "temple-rt: EOF waiting for fd msg",
        ));
    }

    let mut fd_out: Option<OwnedFd> = None;
    let cmsgs = msg
        .cmsgs()
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
    for cmsg in cmsgs {
        if let ControlMessageOwned::ScmRights(mut fds) = cmsg {
            if let Some(fd) = fds.pop() {
                fd_out = Some(unsafe { OwnedFd::from_raw_fd(fd) });
            }
            for fd in fds {
                let _ = unsafe { OwnedFd::from_raw_fd(fd) };
            }
        }
    }

    if bytes_read < Msg::LEN {
        stream.read_exact(&mut buf[bytes_read..])?;
    }

    let decoded = Msg::from_bytes(buf)?;
    Ok((decoded, fd_out))
}
