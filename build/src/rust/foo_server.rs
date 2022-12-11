pub trait FooService {
    fn handle()
}

pub struct FooServer<S> {
    service: S,
}
impl FooServer {
    fn listen(self) {
        let caps = userspace::caps().unwrap();
        let cap = caps
            .iter()
            .find(|c| {
                if let Cap::Listen(l) = c.desc {
                    l.port == b"foo"
                } else {
                    false
                }
            })
            .expect("no listen cap");
        cap.cap_ref.listen().expect("listen failed");
        let mut buf: MaybeUninit<ReqReply> = MaybeUninit::uninit();j
        loop {
            match userspace::recv::<_, MaybeUninit<ReqReply>>(0, &mut buf) {
                Ok(resp) => {
                    if let Some(cap) = resp.cap {
                        match resp.body {
                            userspace::RecvRespBody::Copy(len) => {
                                if len != core::mem::size_of::<ReqReply>() {
                                    error!("invalid len");
                                    continue;
                                }
                            }
                            userspace::RecvRespBody::Page(mut buf) => {
                            }
                        }
                    } else {
                        error!("no reply cap")
                    }
                }
                Err(err) => {
                    defmt::error!("syscall err: {:?}", err);
                }
            }
        }
    }

    fn handle(&mut self, req: MaybeUninit<ReqReply>) {

    }
}

#[derive(bytecheck::Bytecheck)]
pub enum ReqReply<A, R> {
    Req(A),
    Ret(R),
}

enum Req {
    Test(Bar)
}

enum Ret {
    Test(Foo)
}

struct SafeUninit<T>(MaybeUnint<T>);
