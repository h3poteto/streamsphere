import { setup } from "./connection.ts";

document.querySelector<HTMLDivElement>("#app")!.innerHTML = `
  <div>
    <h1>Rheomesh</h1>
    <div class="card">
      <button id="connect" type="button">Connect</button>
      <button id="start" type="button">Publish</button>
      <button id="stop" type="button">Stop</button>
    </div>
    <div>
      <input name="data" id="data" type="text" />
      <button id="send">Send</button>
    </div>
    <div>
      <span id="remote_data"></span>
    </div>
  </div>
`;

setup();
