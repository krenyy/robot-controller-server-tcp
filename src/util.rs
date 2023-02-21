use thiserror::Error;

pub(crate) const SERVER_KEYS: [u16; 5] = [23019, 32037, 18789, 16443, 18189];
pub(crate) const CLIENT_KEYS: [u16; 5] = [32037, 29295, 13603, 29533, 21952];

#[derive(Debug)]
pub(crate) enum ServerMessage {
    Confirmation(u16),
    Move,
    TurnLeft,
    TurnRight,
    PickUp,
    Logout,
    KeyRequest,
    OK,
    LoginFailed,
    SyntaxError,
    LogicError,
    KeyOutOfRangeError,
}

impl ToString for ServerMessage {
    fn to_string(&self) -> String {
        format!(
            "{}\x07\x08",
            match self {
                ServerMessage::Confirmation(x) => x.to_string(),
                ServerMessage::Move => "102 MOVE".to_owned(),
                ServerMessage::TurnLeft => "103 TURN LEFT".to_owned(),
                ServerMessage::TurnRight => "104 TURN RIGHT".to_owned(),
                ServerMessage::PickUp => "105 GET MESSAGE".to_owned(),
                ServerMessage::Logout => "106 LOGOUT".to_owned(),
                ServerMessage::KeyRequest => "107 KEY REQUEST".to_owned(),
                ServerMessage::OK => "200 OK".to_owned(),
                ServerMessage::LoginFailed => "300 LOGIN FAILED".to_owned(),
                ServerMessage::SyntaxError => "301 SYNTAX ERROR".to_owned(),
                ServerMessage::LogicError => "302 LOGIC ERROR".to_owned(),
                ServerMessage::KeyOutOfRangeError => "303 KEY OUT OF RANGE".to_owned(),
            }
        )
    }
}

#[derive(Debug)]
pub(crate) enum ClientMessage {
    String(String),
    Number(usize),
    Ok(i32, i32),
    Recharging,
    FullPower,
}

impl ClientMessage {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s {
            "RECHARGING" => Some(ClientMessage::Recharging),
            "FULL POWER" => Some(ClientMessage::FullPower),
            s => {
                if !s.is_ascii() {
                    return None;
                }
                if s.starts_with("OK ") {
                    let mut split = s.split(' ').skip(1);
                    let (Ok(x), Ok(y)) = (
                        split.next().unwrap().parse::<i32>(),
                        split.next().unwrap().parse::<i32>()
                    ) else {
                        return Some(ClientMessage::String(s.to_string()));
                    };
                    if split.count() > 0 {
                        return Some(ClientMessage::String(s.to_string()));
                    }
                    return Some(ClientMessage::Ok(x, y));
                }
                if let Ok(x) = s.parse::<usize>() {
                    return Some(ClientMessage::Number(x));
                }
                Some(ClientMessage::String(s.to_string()))
            }
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum MessageReceivedError {
    #[error("message is too long!")]
    TooLong,
    #[error("message timed out!")]
    TimedOut,
    #[error("message was invalid!")]
    Invalid,
    #[error("io error")]
    IOError(std::io::Error),
}
