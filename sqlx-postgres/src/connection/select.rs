use super::prepare::Prepare;
use futures::{stream, Stream};
use sqlx_postgres_protocol::{self as proto, DataRow, Execute, Message, Sync};
use std::io;

impl<'a> Prepare<'a> {
    pub fn select(self) -> impl Stream<Item = Result<DataRow, io::Error>> + 'a + Unpin {
        proto::bind::trailer(
            &mut self.connection.wbuf,
            self.bind_state,
            self.bind_values,
            &[],
        );

        self.connection.send(Execute::new("", 0));
        self.connection.send(Sync);

        // FIXME: Manually implement Stream on a new type to avoid the unfold adapter
        stream::unfold(self.connection, |conn| {
            Box::pin(async {
                if !conn.wbuf.is_empty() {
                    if let Err(e) = conn.flush().await {
                        return Some((Err(e), conn));
                    }
                }

                loop {
                    let message = match conn.receive().await {
                        Ok(Some(message)) => message,
                        // FIXME: This is an end-of-file error. How we should bubble this up here?
                        Ok(None) => unreachable!(),
                        Err(e) => return Some((Err(e), conn)),
                    };

                    match message {
                        Message::BindComplete | Message::ParseComplete => {
                            // Indicates successful completion of a phase
                        }

                        Message::DataRow(row) => {
                            break Some((Ok(row), conn));
                        }

                        Message::CommandComplete(_) => {}

                        Message::ReadyForQuery(_) => {
                            // Successful completion of the whole cycle
                            break None;
                        }

                        message => {
                            unimplemented!("received {:?} unimplemented message", message);
                        }
                    }
                }
            })
        })
    }
}
