use std::{collections::HashMap, ops::Index};

use dsys::{protocol::Composite, App, NodeEffect, Protocol};
use sha2::Digest;

use crate::{Message, Multicast, MulticastCrypto, Reply, Request};

pub struct Replica {
    id: u8,
    f: usize,
    pub log: Vec<LogEntry>,
    multicast_seq: u32, // current working sequence number
    spec_num: u32,
    multicast_signatures: HashMap<u32, MulticastSignature>,
    reorder_request: HashMap<u32, Vec<(Multicast, Request)>>,
    app: App,
    replies: HashMap<u32, Reply>,
    complete_count: usize,
    tick_count: usize,
}

enum MulticastSignature {
    SipHash(HashMap<u8, [u8; 4]>),
    P256([u8; 32], [u8; 32]),
}

#[allow(unused)]
pub struct LogEntry {
    request: Request,
    // the link hash that should appear in next multicast, if it links
    next_link: [u8; 32],
}

impl Replica {
    pub fn new(id: u8, app: App, f: usize) -> Self {
        Self {
            id,
            f,
            log: Default::default(),
            multicast_seq: 1,
            spec_num: 0,
            multicast_signatures: Default::default(),
            reorder_request: Default::default(),
            app,
            replies: Default::default(),
            complete_count: 0,
            tick_count: 0,
        }
    }
}

struct I<'a>(&'a [LogEntry]);

impl Index<u32> for I<'_> {
    type Output = LogEntry;

    fn index(&self, index: u32) -> &Self::Output {
        &self.0[(index - 1) as usize]
    }
}

type Event = dsys::NodeEvent<Message>;
type Effect = Vec<dsys::NodeEffect<Message>>;

impl Protocol<Event> for Replica {
    type Effect = Effect;

    fn update(&mut self, event: Event) -> Self::Effect {
        let Event::Handle(message) = event else {
            // TODO move this into `Drop`
            self.tick_count += 1;
            if self.tick_count % 500 == 0 {
                self.report()
            }
            return Effect::NOP;
        };
        match message {
            Message::OrderedRequest(multicast, request) => self.insert_request(multicast, request),
            _ => Effect::NOP,
        }
    }
}

impl Replica {
    fn next_entry(&self) -> u32 {
        self.log.len() as u32 + 1
    }

    fn multicast_verified(&self, seq: u32) -> bool {
        match self.multicast_signatures.get(&seq) {
            None => false,
            Some(MulticastSignature::SipHash(signatures)) => signatures.len() == 3 * self.f + 1,
            Some(MulticastSignature::P256(_, _)) => true,
        }
    }

    fn insert_request(&mut self, multicast: Multicast, request: Request) -> Effect {
        if multicast.seq != self.multicast_seq {
            self.reorder_request
                .entry(multicast.seq)
                .or_default()
                .push((multicast, request));
            return Effect::NOP;
        }

        let mut effect = self.handle_request(multicast, request);
        while let Some(messages) = self.reorder_request.remove(&(self.multicast_seq)) {
            for (multicast, request) in messages {
                effect = effect.compose(self.handle_request(multicast, request));
            }
        }
        effect
    }

    fn handle_request(&mut self, multicast: Multicast, request: Request) -> Effect {
        assert_eq!(multicast.seq, self.multicast_seq);

        use MulticastSignature::*;
        match multicast.crypto {
            MulticastCrypto::SipHash { index, signatures } => {
                // println!("{index} {signatures:02x?}");
                if multicast.seq < self.next_entry()
                    && request != I(&self.log)[multicast.seq].request
                {
                    eprintln!("multicast request mismatch");
                    return Effect::NOP;
                }

                if multicast.seq == self.next_entry() {
                    self.log.push(LogEntry {
                        request,
                        next_link: Default::default(),
                    });
                }

                let SipHash(multicast_signatures) = self.multicast_signatures
                    .entry(multicast.seq)
                    .or_insert(SipHash(Default::default()))
                else {
                    unreachable!()
                };
                for j in index..u8::min(index + 4, (3 * self.f + 1) as _) {
                    multicast_signatures.insert(j, signatures[(j - index) as usize]);
                }
            }
            MulticastCrypto::P256 {
                link_hash,
                signature,
            } => {
                let prev_link = if multicast.seq == 1 {
                    Default::default()
                } else {
                    I(&self.log)[multicast.seq - 1].next_link
                };
                let next_link =
                    sha2::Sha256::digest(&[&multicast.digest[..], &prev_link[..]].concat()).into();
                match (link_hash, signature) {
                    (None, Some(signature)) => {
                        self.multicast_signatures
                            .insert(multicast.seq, P256(signature.0, signature.1));
                        self.log.push(LogEntry { request, next_link });
                    }
                    (Some(link_hash), None) => {
                        if link_hash != prev_link {
                            eprintln!("malformed (link hash)");
                            return Effect::NOP;
                        }
                        self.log.push(LogEntry { request, next_link });
                        self.multicast_seq = multicast.seq + 1;
                    }
                    _ => unreachable!(),
                }
            }
        }

        if !self.multicast_verified(multicast.seq) {
            return Effect::NOP;
        }
        self.multicast_seq = multicast.seq + 1;

        // dbg!(&request);
        // println!("complete");
        self.complete_count += 1;
        let mut effect = Effect::NOP;
        for op_num in self.spec_num + 1..=multicast.seq {
            let request = &I(&self.log)[op_num].request;
            match self.replies.get_mut(&request.client_id) {
                Some(reply) if reply.request_num > request.request_num => return Effect::NOP,
                Some(reply) if reply.request_num == request.request_num => {
                    reply.seq = op_num;
                    effect = effect.compose(Effect::pure(NodeEffect::Send(
                        request.client_addr,
                        Message::Reply(reply.clone()),
                    )));
                    continue;
                }
                _ => {}
            }

            let result = self.app.execute(&request.op);
            let reply = Reply {
                request_num: request.request_num,
                replica_id: self.id,
                result,
                seq: op_num,
            };
            // dbg!(&reply);
            self.replies.insert(request.client_id, reply.clone());
            effect = effect.compose(Effect::pure(NodeEffect::Send(
                request.client_addr,
                Message::Reply(reply),
            )))
        }
        self.spec_num = multicast.seq;
        effect
    }

    fn report(&self) {
        println!(
            "average multicast complete batch size {}",
            self.log.len() as f32 / self.complete_count as f32
        );
        println!("reorder sequence count {}", self.reorder_request.len());
    }
}
