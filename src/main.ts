import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

let ipInput: HTMLInputElement;
let connectBtn: HTMLButtonElement;
let disconnectBtn: HTMLButtonElement;
let connectionStatus: HTMLElement;
let fileNameLabel: HTMLElement;
let browseBtn: HTMLButtonElement;
let cfInput: HTMLInputElement;
let fsInput: HTMLInputElement;
let ampInput: HTMLInputElement;
let playBtn: HTMLButtonElement;
let stopBtn: HTMLButtonElement;
let logArea: HTMLElement;

let isConnected = false;
let wfmLoaded = false;

interface WaveformInfo {
  file_name: string;
  file_size: number;
  sample_count: number;
}

function log(msg: string, type: "info" | "error" | "success" = "info") {
  const time = new Date().toLocaleTimeString();
  const entry = document.createElement("div");
  entry.className = type === "error" ? "log-error" : type === "success" ? "log-success" : "log-entry";
  entry.textContent = `[${time}] ${msg}`;
  logArea.appendChild(entry);
  logArea.scrollTop = logArea.scrollHeight;
}

function updateUI() {
  connectBtn.disabled = isConnected;
  disconnectBtn.disabled = !isConnected;
  ipInput.disabled = isConnected;
  playBtn.disabled = !isConnected || !wfmLoaded;
  stopBtn.disabled = !isConnected;
}

async function connect() {
  const ip = ipInput.value.trim();
  if (!ip) {
    log("Please enter an IP address", "error");
    return;
  }

  connectBtn.disabled = true;
  log(`Connecting to ${ip}...`);

  try {
    const idn = await invoke<string>("connect_instrument", { ip });
    isConnected = true;
    connectionStatus.textContent = `Connected: ${idn}`;
    connectionStatus.className = "status connected";
    log(`Connected: ${idn}`, "success");
  } catch (e) {
    log(`Connection failed: ${e}`, "error");
    connectionStatus.textContent = "Connection failed";
    connectionStatus.className = "status error";
  }

  updateUI();
}

async function disconnect() {
  try {
    await invoke("disconnect_instrument");
    isConnected = false;
    connectionStatus.textContent = "Disconnected";
    connectionStatus.className = "status";
    log("Disconnected");
  } catch (e) {
    log(`Disconnect error: ${e}`, "error");
  }

  updateUI();
}

async function browse() {
  const selected = await open({
    multiple: false,
    filters: [
      { name: "Waveform Files", extensions: ["WAVEFORM", "waveform"] },
      { name: "All Files", extensions: ["*"] },
    ],
  });

  if (!selected) return;

  const filePath = selected as string;
  const fileName = filePath.split(/[/\\]/).pop() || filePath;
  fileNameLabel.textContent = fileName;
  log(`Loading file: ${fileName}...`);

  try {
    const info = await invoke<WaveformInfo>("load_waveform", { filePath });
    wfmLoaded = true;
    log(`Loaded: ${info.file_name} (${info.sample_count} IQ samples, ${info.file_size} bytes)`, "success");
  } catch (e) {
    log(`Failed to load waveform: ${e}`, "error");
    fileNameLabel.textContent = "No file selected";
    wfmLoaded = false;
  }

  updateUI();
}

async function play() {
  const cf = parseFloat(cfInput.value) * 1e6;
  const fs = parseFloat(fsInput.value) * 1e6;
  const amp = parseFloat(ampInput.value);

  if (isNaN(cf) || isNaN(fs) || isNaN(amp)) {
    log("Invalid configuration values", "error");
    return;
  }

  playBtn.disabled = true;
  log(`Playing waveform (CF=${cfInput.value} MHz, FS=${fsInput.value} MHz, Power=${ampInput.value} dBm)...`);

  try {
    await invoke("play_waveform", { cf, fs, amp });
    log("Waveform playing", "success");
  } catch (e) {
    log(`Play failed: ${e}`, "error");
  }

  updateUI();
}

async function stop() {
  stopBtn.disabled = true;
  log("Stopping waveform...");

  try {
    await invoke("stop_waveform");
    log("Waveform stopped", "success");
  } catch (e) {
    log(`Stop failed: ${e}`, "error");
  }

  updateUI();
}

window.addEventListener("DOMContentLoaded", () => {
  ipInput = document.querySelector("#ip-input")!;
  connectBtn = document.querySelector("#connect-btn")!;
  disconnectBtn = document.querySelector("#disconnect-btn")!;
  connectionStatus = document.querySelector("#connection-status")!;
  fileNameLabel = document.querySelector("#file-name")!;
  browseBtn = document.querySelector("#browse-btn")!;
  cfInput = document.querySelector("#cf-input")!;
  fsInput = document.querySelector("#fs-input")!;
  ampInput = document.querySelector("#amp-input")!;
  playBtn = document.querySelector("#play-btn")!;
  stopBtn = document.querySelector("#stop-btn")!;
  logArea = document.querySelector("#log-area")!;

  connectBtn.addEventListener("click", connect);
  disconnectBtn.addEventListener("click", disconnect);
  browseBtn.addEventListener("click", browse);
  playBtn.addEventListener("click", play);
  stopBtn.addEventListener("click", stop);

  updateUI();
  log("Application ready");
});
