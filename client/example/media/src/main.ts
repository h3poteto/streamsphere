import { setup } from "./connection.ts";

document.querySelector<HTMLDivElement>("#app")!.innerHTML = `
  <div>
    <h1>Rheomesh</h1>
    <video id="localVideo" playsinline autoplay muted width="480"></video>
    <video id="remoteVideo" playsinline autoplay width="480"></video>
    <audio id="remoteAudio" controls autoplay></audio>
    <div class="card">
      <button id="connect" type="button">Connect</button>
      <button id="capture" type="button">Capture</button>
      <button id="mic" type="button">Mic</button>
      <button id="stop" type="button">Stop</button>
    </div>
    <p class="read-the-docs">
      Click on the Vite and TypeScript logos to learn more
    </p>
  </div>
`;

setup();
