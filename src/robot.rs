use crate::util::{ClientMessage, MessageReceivedError, ServerMessage, CLIENT_KEYS, SERVER_KEYS};
use async_recursion::async_recursion;
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

#[derive(Debug)]
enum Position {
    Unknown,
    Known(i32, i32),
}

#[derive(Debug)]
enum Direction {
    Unknown,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug)]
enum MoveResult {
    Ok,
    Rammed,
}

#[derive(Debug)]
struct Robot {
    position: Position,
    direction: Direction,
}

pub(crate) struct RobotController {
    socket: TcpStream,
    robot: Robot,
}

impl RobotController {
    pub(crate) async fn start(socket: TcpStream) {
        tracing::info!("connected!");
        Self {
            socket,
            robot: Robot {
                position: Position::Unknown,
                direction: Direction::Unknown,
            },
        }
        .run()
        .await;
        tracing::info!("disconnected!");
    }

    async fn send(&mut self, msg: ServerMessage) -> Option<()> {
        if let Err(e) = self.socket.write_all(msg.to_string().as_bytes()).await {
            tracing::error!("connection interrupted! ({e:?})");
            return None;
        };
        Some(())
    }

    async fn receive<const MAX_LENGTH: usize, const TIMEOUT_SECONDS: u64>(
        &mut self,
    ) -> Result<ClientMessage, MessageReceivedError> {
        const SEP: &str = "\x07\x08";
        const SEP_LEN: usize = SEP.len();

        let mut data = [0u8; 256];
        let mut i = 0usize;

        loop {
            match tokio::time::timeout(
                Duration::from_secs(TIMEOUT_SECONDS),
                self.socket.read(&mut data[i..i + 1]),
            )
            .await
            {
                Ok(res) => {
                    if let Err(e) = res {
                        tracing::error!("err: {e:?}");
                        return Err(MessageReceivedError::IOError(e));
                    }
                }
                Err(_e) => {
                    tracing::error!("timeout exceeded!");
                    return Err(MessageReceivedError::TimedOut);
                }
            }

            if i >= SEP_LEN && core::str::from_utf8(&data[i - (SEP_LEN - 1)..=i]).unwrap() == SEP {
                break;
            }

            i += 1;

            if i == MAX_LENGTH {
                return Err(MessageReceivedError::TooLong);
            }
        }

        ClientMessage::parse(core::str::from_utf8(&data[0..i - 1]).unwrap())
            .ok_or(MessageReceivedError::Invalid)
    }

    async fn wait_for_recharging(&mut self) -> Option<()> {
        match tokio::time::timeout(Duration::from_secs(5), self.receive::<12, 5>()).await {
            Ok(Ok(msg)) => match msg {
                ClientMessage::FullPower => Some(()),
                _ => {
                    self.send(ServerMessage::LogicError).await;
                    None
                }
            },
            Err(_) | Ok(Err(MessageReceivedError::TimedOut)) => {
                tracing::error!("recharging timed out!");
                None
            }
            Ok(Err(_)) => {
                self.send(ServerMessage::SyntaxError).await;
                None
            }
        }
    }

    #[async_recursion]
    async fn get<const MAX_LENGTH: usize>(&mut self) -> Option<ClientMessage> {
        match self.receive::<MAX_LENGTH, 1>().await {
            Ok(msg) => match msg {
                ClientMessage::Recharging => {
                    if self.wait_for_recharging().await.is_none() {
                        return None;
                    }
                    self.get::<MAX_LENGTH>().await
                }
                ClientMessage::FullPower => {
                    self.send(ServerMessage::LogicError).await;
                    None
                }
                _ => Some(msg),
            },
            Err(e) => match e {
                MessageReceivedError::TimedOut => return None,
                _ => {
                    tracing::error!("{e}");
                    self.send(ServerMessage::SyntaxError).await;
                    None
                }
            },
        }
    }

    async fn log_in(&mut self) -> Option<()> {
        let msg = self.get::<20>().await?;
        let ClientMessage::String(name) = msg else {
            tracing::error!("wrong variant received: {msg:?}");
            self.send(ServerMessage::SyntaxError).await;
            return None;
        };
        tracing::debug!("name: {name:?}");

        self.send(ServerMessage::KeyRequest).await;

        let msg = self.get::<12>().await?;
        let ClientMessage::Number(key_id) = msg else {
            tracing::error!("wrong variant received: {msg:?}");
            self.send(ServerMessage::SyntaxError).await;
            return None;
        };
        tracing::debug!("key_id: {key_id}");

        if key_id > 4 {
            tracing::info!("key_id: {key_id} is out of range, disconnecting...");
            self.send(ServerMessage::KeyOutOfRangeError).await;
            return None;
        }

        let server_key = SERVER_KEYS[key_id];
        let client_key = CLIENT_KEYS[key_id];

        let name_char_sum: u16 = name.into_bytes().into_iter().map(|x| x as u16).sum();
        let checksum = name_char_sum.wrapping_mul(1000);
        let server_checksum = checksum.wrapping_add(server_key);
        self.send(ServerMessage::Confirmation(server_checksum))
            .await;

        let msg = self.get::<12>().await?;
        let ClientMessage::Number(client_checksum) = msg else {
            tracing::error!("wrong variant received: {msg:?}");
            self.send(ServerMessage::SyntaxError).await;
            return None;
        };
        let Ok(client_checksum): Result<u16,_> = client_checksum.try_into() else {
                tracing::error!("invalid client checksum!");
                self.send(ServerMessage::SyntaxError).await;
                return None;
        };
        if checksum != client_checksum.wrapping_sub(client_key) {
            self.send(ServerMessage::LoginFailed).await;
            return None;
        }

        self.send(ServerMessage::OK).await;
        Some(())
    }

    async fn pick_up_secret(&mut self) -> Option<()> {
        self.send(ServerMessage::PickUp).await;
        match self.get::<100>().await? {
            ClientMessage::String(secret) => {
                tracing::debug!("secret found: {secret:?}");
                Some(())
            }
            ClientMessage::Number(secret) => {
                tracing::debug!("secret found: {secret:?}");
                Some(())
            }
            msg => {
                tracing::error!("wrong variant received: {msg:?}");
                self.send(ServerMessage::SyntaxError).await;
                return None;
            }
        }
    }

    async fn log_out(&mut self) {
        tracing::trace!("logging out...");
        self.send(ServerMessage::Logout).await;
        tracing::trace!("logged out!");
    }

    async fn move_forward(&mut self) -> Option<MoveResult> {
        self.send(ServerMessage::Move).await;
        let msg = self.get::<12>().await?;
        let ClientMessage::Ok(new_x, new_y) = msg else {
            tracing::error!("wrong variant received: {msg:?}");
            self.send(ServerMessage::SyntaxError).await;
            return None
        };
        let mut rammed = false;
        if let Position::Known(x, y) = self.robot.position {
            if (new_x - x, new_y - y) == (0, 0) {
                rammed = true;
            }
            if let Direction::Unknown = self.robot.direction {
                self.robot.direction = match (new_x - x, new_y - y) {
                    (0, 0) => Direction::Unknown,
                    (-1, 0) => Direction::Left,
                    (1, 0) => Direction::Right,
                    (0, -1) => Direction::Down,
                    (0, 1) => Direction::Up,
                    _ => unreachable!(),
                }
            }
        }
        self.robot.position = Position::Known(new_x, new_y);
        Some(if rammed {
            MoveResult::Rammed
        } else {
            MoveResult::Ok
        })
    }

    async fn turn_left(&mut self) -> Option<()> {
        self.send(ServerMessage::TurnLeft).await;
        let msg = self.get::<12>().await?;
        let ClientMessage::Ok(x, y) = msg else {
            tracing::error!("wrong variant received: {msg:?}");
            self.send(ServerMessage::SyntaxError).await;
            return None
        };
        self.robot.position = Position::Known(x, y);
        self.robot.direction = match self.robot.direction {
            Direction::Unknown => Direction::Unknown,
            Direction::Up => Direction::Left,
            Direction::Down => Direction::Right,
            Direction::Left => Direction::Down,
            Direction::Right => Direction::Up,
        };
        Some(())
    }

    async fn turn_right(&mut self) -> Option<()> {
        self.send(ServerMessage::TurnRight).await;
        let msg = self.get::<12>().await?;
        let ClientMessage::Ok(x, y) = msg else {
            tracing::error!("wrong variant received: {msg:?}");
            self.send(ServerMessage::SyntaxError).await;
            return None
        };
        self.robot.position = Position::Known(x, y);
        self.robot.direction = match self.robot.direction {
            Direction::Unknown => Direction::Unknown,
            Direction::Up => Direction::Right,
            Direction::Down => Direction::Left,
            Direction::Left => Direction::Up,
            Direction::Right => Direction::Down,
        };
        Some(())
    }

    async fn rotate(&mut self, direction: &Direction) -> Option<()> {
        match (&self.robot.direction, direction) {
            (Direction::Up, Direction::Up)
            | (Direction::Down, Direction::Down)
            | (Direction::Left, Direction::Left)
            | (Direction::Right, Direction::Right) => Some(()),
            (Direction::Up, Direction::Down)
            | (Direction::Down, Direction::Up)
            | (Direction::Left, Direction::Right)
            | (Direction::Right, Direction::Left) => {
                self.turn_left().await?;
                self.turn_left().await?;
                Some(())
            }
            (Direction::Up, Direction::Left)
            | (Direction::Left, Direction::Down)
            | (Direction::Down, Direction::Right)
            | (Direction::Right, Direction::Up) => {
                self.turn_left().await?;
                Some(())
            }
            (Direction::Up, Direction::Right)
            | (Direction::Right, Direction::Down)
            | (Direction::Down, Direction::Left)
            | (Direction::Left, Direction::Up) => {
                self.turn_right().await?;
                Some(())
            }
            (Direction::Unknown, _) | (_, Direction::Unknown) => unreachable!(),
        }
    }

    async fn acquire_initial_state(&mut self) -> Option<()> {
        self.move_forward().await?;
        if let MoveResult::Rammed = self.move_forward().await? {
            self.turn_left().await?;
            self.move_forward().await?;
        }
        Some(())
    }

    pub(crate) async fn run(&mut self) -> Option<()> {
        self.log_in().await?;

        self.acquire_initial_state().await?;
        tracing::trace!("initial state retrieved!");

        loop {
            tracing::trace!("{:?}", self.robot);
            let Position::Known(x, y) = self.robot.position else { unreachable!() };
            if x != 0 {
                let direction = if x.is_negative() {
                    Direction::Right
                } else {
                    Direction::Left
                };
                let avoid_direction = if y.is_negative() {
                    Direction::Up
                } else {
                    Direction::Down
                };
                self.rotate(&direction).await?;
                if let MoveResult::Rammed = self.move_forward().await? {
                    tracing::trace!("avoiding obstacle!");
                    self.rotate(&avoid_direction).await?;
                    self.move_forward().await?;
                    self.rotate(&direction).await?;
                    self.move_forward().await?;
                }
                continue;
            }
            if y != 0 {
                let direction = if y.is_negative() {
                    Direction::Up
                } else {
                    Direction::Down
                };
                let avoid_direction = if x.is_negative() {
                    Direction::Right
                } else {
                    Direction::Left
                };
                self.rotate(&direction).await?;
                if let MoveResult::Rammed = self.move_forward().await? {
                    tracing::trace!("avoiding obstacle!");
                    self.rotate(&avoid_direction).await?;
                    self.move_forward().await?;
                    self.rotate(&direction).await?;
                    self.move_forward().await?;
                }
                continue;
            }
            break;
        }

        self.pick_up_secret().await?;
        self.log_out().await;
        Some(())
    }
}
