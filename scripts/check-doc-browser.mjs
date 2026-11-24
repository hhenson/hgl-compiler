// Run the SDOC-7 browser cases against the generated doc_html fixture site:
// node scripts/check-doc-browser.mjs SITE
// Node 22 and a local Chromium or Chrome binary (CHROME) suffice.
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as pause } from 'node:timers/promises';
import { pathToFileURL } from 'node:url';
import assert from 'node:assert/strict';

const waitSeconds = 60;
const profile = await mkdtemp(join(process.env.RUNNER_TEMP ?? tmpdir(), 'hgl-doc-browser-'));
const site = process.argv[2];
const chrome = spawn(process.env.CHROME || 'google-chrome', [
  '--headless', '--disable-gpu', '--disable-background-networking', '--no-first-run',
  '--remote-debugging-port=0', '--user-data-dir=' + profile, 'about:blank',
], { stdio: ['ignore', 'pipe', 'pipe'] });
let chromeOutput = '';
for (const stream of [chrome.stdout, chrome.stderr]) {
  stream.setEncoding('utf8');
  stream.on('data', chunk => {
    chromeOutput = (chromeOutput + chunk).slice(-16_000);
  });
}
let launchError;
chrome.on('error', error => { launchError = error; });
const chromeClosed = new Promise(resolve => chrome.once('close', resolve));

async function waitFor(action, probe) {
  let timedOut = false;
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => {
      timedOut = true;
      reject(Error(`Chrome did not ${action} within ${waitSeconds} seconds.`));
    }, waitSeconds * 1000);
  });
  async function poll() {
    while (!timedOut) {
      if (launchError) throw Error(`Chrome could not start: ${launchError.message}`, { cause: launchError });
      if (chrome.exitCode !== null || chrome.signalCode !== null) {
        await chromeClosed;
        throw Error(`Chrome exited (${chrome.signalCode ?? chrome.exitCode}) before it could ${action}.`);
      }
      const ready = await probe();
      if (ready) return ready;
      await pause(50);
    }
  }
  try {
    return await Promise.race([poll(), timeout]);
  } finally {
    clearTimeout(timer);
  }
}

let socket;
const history = [];
try {
  const port = await waitFor('report a DevTools port', async () => {
    try {
      const [port] = (await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).split('\n');
      if (/^\d+$/.test(port)) return port;
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
    }
  });
  const tab = await waitFor('report a DevTools page', async () => {
    const tabs = await (await fetch(`http://127.0.0.1:${port}/json/list`, {
      signal: AbortSignal.timeout(waitSeconds * 1000),
    })).json();
    return tabs.find(candidate => candidate.type === 'page');
  });
  socket = new WebSocket(tab.webSocketDebuggerUrl);
  await waitFor('open its DevTools connection', () => {
    if (socket.readyState === WebSocket.CLOSED) throw Error('Chrome closed its DevTools connection.');
    return socket.readyState === WebSocket.OPEN;
  });
  socket.addEventListener('message', ({data}) => {
    const message = JSON.parse(data);
    if (message.method) history.push(message);
  });
  let nextId = 0;
  async function send(method, params = {}) {
    const id = ++nextId;
    return new Promise((resolve, reject) => {
      const listener = ({ data }) => {
        const reply = JSON.parse(data);
        if (reply.id !== id) return;
        socket.removeEventListener('message', listener);
        if (reply.error) reject(Error(JSON.stringify(reply.error)));
        else resolve(reply.result);
      };
      socket.addEventListener('message', listener);
      socket.send(JSON.stringify({id, method, params}));
    });
  }
  async function evaluate(expression) {
    const result = await send('Runtime.evaluate', {expression, returnByValue: true});
    if (result.exceptionDetails) throw Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
  }
  await send('Runtime.enable');
  await send('Log.enable');
  await send('Network.enable');
  await send('Network.emulateNetworkConditions', {
    offline: true, latency: 0, downloadThroughput: 0, uploadThroughput: 0,
  });
  await send('Emulation.setDeviceMetricsOverride', {
    width: 390, height: 850, deviceScaleFactor: 1, mobile: true,
  });
  await send('Emulation.setEmulatedMedia', {
    features: [{name: 'prefers-color-scheme', value: 'dark'}],
  });
  await send('Page.navigate', { url: pathToFileURL(join(site, 'modules/mod-guide.api.html')).href });
  await waitFor('render the formulas and diagrams', () => evaluate(`Boolean(
    document.querySelector('.formula math') &&
    document.querySelectorAll('.formula-block math').length === 2 &&
    document.querySelectorAll('.diagram svg').length === 3
  )`));
  const rendered = await evaluate(`({
    inline: document.querySelector('.formula math').textContent,
    formulas: [...document.querySelectorAll('.formula-block math annotation')].map(formula => formula.textContent.trim()),
    diagrams: [...document.querySelectorAll('.diagram svg')].map(svg => ({
      text: svg.textContent,
      usesHtml: Boolean(svg.querySelector('foreignObject')),
      links: [...svg.querySelectorAll('a')].map(link => link.getAttribute('href') || link.getAttribute('xlink:href')),
    })),
    formulaFallbacks: [...document.querySelectorAll('code.language-math')].map(code => code.textContent.trim()),
    diagramFallback: document.querySelector('code.language-mermaid').textContent.trim(),
    unhandled: document.querySelector('code.formula-inline'),
  })`);
  assert.match(rendered.inline, /shared/);
  assert.match(rendered.diagrams[0].text, /A/);
  assert.match(rendered.diagrams[0].text, /B/);
  const protectedDiagram = rendered.diagrams[1];
  assert.match(protectedDiagram.text, /bold/);
  assert.equal(protectedDiagram.usesHtml, false);
  assert.doesNotMatch(protectedDiagram.text, /hgl-theme-override|hgl-font-override|#010203/i);
  assert.ok(protectedDiagram.links.every(link => link === null || link.startsWith('#')));
  const journey = rendered.diagrams[2];
  assert.match(journey.text, /Read notes/);
  assert.equal(journey.usesHtml, false);
  assert.deepEqual(rendered.formulas, ['a^2+b^2=c^2', '\\kern{-1em}']);
  assert.deepEqual(rendered.formulaFallbacks, [
    '\\badHglCommand{b}',
    '\\rule[-100000em]{1em}{1em}',
    '\\kern{-100000em}',
    '\\raisebox{100000em}{x}',
  ]);
  assert.equal(rendered.diagramFallback, 'not a diagram');
  assert.equal(rendered.unhandled, null);
  const layout = await evaluate(`({
    viewport: window.innerWidth,
    width: document.documentElement.scrollWidth,
    dark: matchMedia('(prefers-color-scheme: dark)').matches,
  })`);
  assert.deepEqual(layout, {viewport: 390, width: 390, dark: true});

  await send('Page.navigate', { url: pathToFileURL(join(site, 'index.html')).href });
  await waitFor('load the declaration search index', () =>
    evaluate('document.querySelector("#declaration-search")?.hidden === false'));
  const search = await evaluate(`(() => {
    const query = document.querySelector('#declaration-query');
    const status = document.querySelector('[role=status]');
    const find = term => {
      query.value = term;
      query.dispatchEvent(new Event('input'));
      return Array.from(document.querySelectorAll('#search-results a'), link => [link.textContent, link.getAttribute('href')]);
    };
    const visible = find('INCREMENT');
    const hidden = find('private_marker');
    const empty = find('');
    return { visible, hidden, empty, status: status.textContent };
  })()`);
  assert.deepEqual(search, {
    visible: [['guide.api::increment', 'modules/mod-guide.api.html#decl-increment']],
    hidden: [], empty: [], status: '',
  });
  const requested = history.filter(({method}) => method === 'Network.requestWillBeSent');
  assert.ok(requested.some(({params}) => params.request.url.endsWith('/katex.min.js')));
  assert.ok(requested.some(({params}) => params.request.url.endsWith('/mermaid.min.js')));
  assert.ok(requested.every(({params}) => params.request.url.startsWith('file:')), 'network request');
  console.log('Offline browser checks passed for math, plain Mermaid labels, secure diagram settings, source fallbacks, search, and local assets.');
} catch (error) {
  console.error(`Chrome output:\n${chromeOutput.trim() || '(none)'}`);
  const messages = history.filter(({method}) => method === 'Runtime.exceptionThrown' || method === 'Log.entryAdded');
  if (messages.length) console.error(messages.slice(-15));
  throw error;
} finally {
  socket?.close();
  chrome.kill();
  const forceExit = setTimeout(() => {
    chrome.kill('SIGKILL');
    chrome.stdout.destroy();
    chrome.stderr.destroy();
  }, 5000);
  try {
    await chromeClosed;
  } finally {
    clearTimeout(forceExit);
  }
  await rm(profile, { recursive: true, force: true, maxRetries: 5 });
}
