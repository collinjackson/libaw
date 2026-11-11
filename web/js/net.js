// A libaw-server connection (docs/socket-api.md). Works in browsers and in Node 22+ (global
// WebSocket). request() matches the server's direct reply by `id`; everything else (state,
// event, turn, game_over, and errors without an id) goes to onMessage.
export class Conn {
  constructor(url) {
    this.url = url;
    this.nextId = 1;
    this.waiting = new Map();
    this.onMessage = () => {};
    this.onClose = () => {};
  }

  open() {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(this.url);
      this.ws = ws;
      ws.onopen = () => resolve(this);
      ws.onerror = () => reject(new Error(`could not connect to ${this.url}`));
      ws.onclose = () => {
        for (const { reject: rj } of this.waiting.values()) rj(new Error('connection closed'));
        this.waiting.clear();
        this.onClose();
      };
      ws.onmessage = ({ data }) => {
        const m = JSON.parse(data);
        if (m.id != null && this.waiting.has(m.id)) {
          const { resolve: rs, reject: rj } = this.waiting.get(m.id);
          this.waiting.delete(m.id);
          if (m.type === 'error') rj(Object.assign(new Error(m.message), { reply: m }));
          else rs(m);
        }
        // Replies are passed on too: `joined` and `ok` are followed by broadcasts the UI wants
        // in order, and a client may want to log them.
        this.onMessage(m);
      };
    });
  }

  send(msg) {
    this.ws.send(JSON.stringify(msg));
  }

  request(msg) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.waiting.set(id, { resolve, reject });
      this.send({ ...msg, id });
    });
  }

  close() {
    this.ws?.close();
  }
}
