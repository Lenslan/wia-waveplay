import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

let ipInput: HTMLInputElement;
let connectBtn: HTMLButtonElement;
let disconnectBtn: HTMLButtonElement;
let connectionStatus: HTMLElement;
let fileNameLabel: HTMLElement;
let browseBtn: HTMLButtonElement;
let exportBtn: HTMLButtonElement;
let cfInput: HTMLInputElement;
let bwInput: HTMLInputElement;
let frameIntervalInput: HTMLInputElement;
let ampInput: HTMLInputElement;
let playBtn: HTMLButtonElement;
let stopBtn: HTMLButtonElement;
let repeatCheck: HTMLInputElement;
let repeatCountInput: HTMLInputElement;
let logArea: HTMLElement;

let isConnected = false;
let wfmLoaded = false;
let isMatSource = false;

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
  exportBtn.disabled = !wfmLoaded || !isMatSource;
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
      { name: "MATLAB Files", extensions: ["mat"] },
      { name: "Waveform Files", extensions: ["WAVEFORM", "waveform"] },
      { name: "All Files", extensions: ["*"] },
    ],
  });

  if (!selected) return;

  const filePath = selected as string;
  const fileName = filePath.split(/[/\\]/).pop() || filePath;
  isMatSource = fileName.toLowerCase().endsWith(".mat");
  fileNameLabel.textContent = fileName;
  log(`Loading file: ${fileName}...`);

  try {
    const bwMhz = parseInt(bwInput.value, 10);
    const frameIntervalUs = parseInt(frameIntervalInput.value, 10);
    if (isNaN(bwMhz) || bwMhz <= 0 || isNaN(frameIntervalUs) || frameIntervalUs < 0) {
      log("Invalid BW or Frame Interval values", "error");
      return;
    }
    const info = await invoke<WaveformInfo>("load_waveform", { filePath, bwMhz, frameIntervalUs });
    wfmLoaded = true;
    log(`Loaded: ${info.file_name} (${info.sample_count} IQ samples, ${info.file_size} bytes)`, "success");
  } catch (e) {
    log(`Failed to load waveform: ${e}`, "error");
    fileNameLabel.textContent = "No file selected";
    wfmLoaded = false;
    isMatSource = false;
  }

  updateUI();
}

async function exportWaveform() {
  const defaultName = (fileNameLabel.textContent || "waveform").replace(/\.mat$/i, ".WAVEFORM");
  const savePath = await save({
    defaultPath: defaultName,
    filters: [
      { name: "Waveform Files", extensions: ["WAVEFORM"] },
    ],
  });

  if (!savePath) return;

  exportBtn.disabled = true;
  log("Exporting waveform...");

  try {
    await invoke("export_waveform", { filePath: savePath });
    const savedName = savePath.split(/[/\\]/).pop() || savePath;
    log(`Exported: ${savedName}`, "success");
  } catch (e) {
    log(`Export failed: ${e}`, "error");
  }

  updateUI();
}

async function play() {
  const cf = parseFloat(cfInput.value) * 1e6;
  const bwMhz = parseFloat(bwInput.value);
  const amp = parseFloat(ampInput.value);

  if (isNaN(cf) || isNaN(bwMhz) || bwMhz <= 0 || isNaN(amp)) {
    log("Invalid configuration values", "error");
    return;
  }

  const repeatCount = repeatCheck.checked ? parseInt(repeatCountInput.value, 10) || 1 : 0;

  playBtn.disabled = true;
  const repeatInfo = repeatCount > 0 ? `Repeat=${repeatCount}` : "Continuous";
  log(`Playing waveform (CF=${cfInput.value} MHz, BW=${bwInput.value} MHz, Power=${ampInput.value} dBm, ${repeatInfo})...`);

  try {
    await invoke("play_waveform", { cf, bwMhz, amp, repeatCount });
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
  exportBtn = document.querySelector("#export-btn")!;
  cfInput = document.querySelector("#cf-input")!;
  bwInput = document.querySelector("#bw-input")!;
  frameIntervalInput = document.querySelector("#frame-interval-input")!;
  ampInput = document.querySelector("#amp-input")!;
  playBtn = document.querySelector("#play-btn")!;
  stopBtn = document.querySelector("#stop-btn")!;
  repeatCheck = document.querySelector("#repeat-check")!;
  repeatCountInput = document.querySelector("#repeat-count")!;
  logArea = document.querySelector("#log-area")!;

  connectBtn.addEventListener("click", connect);
  disconnectBtn.addEventListener("click", disconnect);
  browseBtn.addEventListener("click", browse);
  exportBtn.addEventListener("click", exportWaveform);
  playBtn.addEventListener("click", play);
  stopBtn.addEventListener("click", stop);
  repeatCheck.addEventListener("change", () => {
    repeatCountInput.disabled = !repeatCheck.checked;
  });

  // Channel help popup
  const channelHelp = document.querySelector("#channel-help")!;
  const channelPopup = document.querySelector("#channel-popup")!;

  channelHelp.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    channelPopup.classList.toggle("show");
  });

  document.addEventListener("click", (e) => {
    if (!channelPopup.contains(e.target as Node)) {
      channelPopup.classList.remove("show");
    }
  });

  // Click a table row to fill the frequency
  channelPopup.addEventListener("click", (e) => {
    const td = (e.target as HTMLElement).closest("td");
    if (!td) return;
    const tr = td.closest("tr");
    if (!tr) return;
    const firstTd = tr.querySelector("td");
    if (firstTd) {
      cfInput.value = firstTd.textContent!.trim();
      channelPopup.classList.remove("show");
    }
  });

  updateUI();
  log("Application ready");
});
