extern crate mio;
extern crate futures;
extern crate bounded_spsc_queue;

use std::io;

use futures::{Poll, Async};
use futures::stream::Stream;
use mio::channel::{ctl_pair, SenderCtl, ReceiverCtl};
use bounded_spsc_queue::{Producer, Consumer};

pub struct Sender<T> {
    ctl: SenderCtl,
    inner: Producer<T>,
}

pub struct Receiver<T> {
    ctl: ReceiverCtl,
    inner: Consumer<T>,
}

pub fn pump<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let (p, c) = bounded_spsc_queue::make(capacity);
    let (tx, rx) = ctl_pair();
    let tx = Sender {
        ctl: tx,
        inner: p,
    };
    let rx = Receiver {
        ctl: rx,
        inner: c,
    };
    (tx, rx)
}

impl<T> Sender<T> {
    pub fn send(&self, data: T) -> io::Result<()> {
        self.inner.push(data);
        self.ctl.inc()
    }
}

impl<T> Receiver<T> {
    pub fn recv(&self) -> io::Result<Option<T>> {
        Ok(self.inner.try_pop())
    }
}

// Delegate everything to `self.ctl`
impl<T> mio::Evented for Receiver<T> {
    fn register(&self,
                poll: &mio::Poll,
                token: mio::Token,
                interest: mio::Ready,
                opts: mio::PollOpt)
                -> io::Result<()> {
        self.ctl.register(poll, token, interest, opts)
    }

    fn reregister(&self,
                  poll: &mio::Poll,
                  token: mio::Token,
                  interest: mio::Ready,
                  opts: mio::PollOpt)
                  -> io::Result<()> {
        self.ctl.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        self.ctl.deregister(poll)
    }
}

impl<T> Stream for Receiver<T> {
    type Item = T;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<T>, io::Error> {
        match self.inner.try_pop() {
            None => Ok(Async::NotReady),
            x @ Some(..) => Ok(Async::Ready(x)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use futures::{Future, Async};
    use futures::stream::Stream;

    #[test]
    fn single_thread() {
        let (tx, rx) = pump(3);
        assert!(tx.send(1).is_ok());
        assert!(tx.send(2).is_ok());
        assert!(tx.send(3).is_ok());
        let f = rx.take(3).collect();
        assert_eq!([1, 2, 3], f.wait().unwrap().as_slice());
    }

    #[test]
    fn multi_thread_pop() {
        let (tx, rx) = pump(1);
        let t = thread::spawn(move || {
            assert!(tx.send(1).is_ok());
        });
        assert_eq!(1, rx.inner.pop());
        t.join().unwrap();
    }

    #[test]
    fn multi_thread_stream() {
        let (tx, rx) = pump(1);
        let t = thread::spawn(move || {
            assert!(tx.send(1).is_ok());
        });
        let mut f = rx.take(1).collect();
        loop {
            if let Ok(Async::Ready(x)) = f.poll() {
                assert_eq!([1], x.as_slice());
                break;
            }
        }
        t.join().unwrap();
    }
}
