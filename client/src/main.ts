import { setup } from "./connection.ts";

document.querySelector<HTMLDivElement>("#app")!.innerHTML = `
  <div>
    <h1>Streamsphere</h1>
    <video id="localVideo" playsinline autoplay muted width="480"></video>
    <video id="remoteVideo" playsinline autoplay width="480"></video>
    <div class="card">
      <button id="connect" type="button">Connect</button>
      <button id="capture" type="button">Capture</button>
      <button id="stop" type="button">Stop</button>
    </div>
    <p class="read-the-docs">
      Click on the Vite and TypeScript logos to learn more
    </p>
  </div>
`;

setup();
