// Run the SDOC-7 browser cases against the generated doc_html fixture site:
// node scripts/check-doc-browser.mjs SITE
// Node 22 and a local Chromium or Chrome binary (CHROME) suffice.
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import assert from 'node:assert/strict';

const profile = await mkdtemp(join(process.env.RUNNER_TEMP ?? tmpdir(), 'hgl-doc-browser-'));
const site = process.argv[2];
const chrome = spawn(process.env.CHROME || 'google-chrome', [
  '--headless', '--disable-gpu', '--disable-background-networking', '--no-first-run',
  '--remote-debugging-port=0', '--user-data-dir=' + profile, 'about:blank',
], { stdio: 'ignore' });
const pause = () => new Promise(resolve => setTimeout(resolve, 50));
let socket;
const history = [];
try {
  let port;
  for (let attempt = 0; attempt < 100 && !port; ++attempt) {
    try {
      port = (await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).split('\n')[0];
    } catch {
      await pause();
    }
  }
  assert.ok(port);
  const tabs = await (await fetch('http://127.0.0.1:' + port + '/json/list')).json();
  const tab = tabs.find(candidate => candidate.type === 'page');
  assert.ok(tab);
  socket = new WebSocket(tab.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true });
    socket.addEventListener('error', reject, { once: true });
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
  async function when(expression) {
    for (let attempt = 0; attempt < 120; ++attempt) {
      if (await evaluate(expression)) return;
      await pause();
    }
    throw Error('Browser did not satisfy ' + expression);
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
  await when('Boolean(document.querySelector(".formula math") && document.querySelectorAll(".formula-block math").length === 2 && document.querySelectorAll(".diagram svg").length === 3)');
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
  await when('Boolean(document.querySelector("#declaration-search")?.hidden === false)');
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
    visible: [['guide.api::increment', 'modules/mod-guide.api.html#decl-increment-0']],
    hidden: [], empty: [], status: '',
  });
  const requested = history.filter(({method}) => method === 'Network.requestWillBeSent');
  assert.ok(requested.some(({params}) => params.request.url.endsWith('/katex.min.js')));
  assert.ok(requested.some(({params}) => params.request.url.endsWith('/mermaid.min.js')));
  assert.ok(requested.every(({params}) => params.request.url.startsWith('file:')), 'network request');
  console.log('Offline browser checks passed for math, plain Mermaid labels, secure diagram settings, source fallbacks, search, and local assets.');
} catch (error) {
  console.error(history.filter(({method}) => method === 'Runtime.exceptionThrown' || method === 'Log.entryAdded').slice(-15));
  throw error;
} finally {
  socket?.close();
  if (chrome.exitCode === null && chrome.signalCode === null) {
    const closed = new Promise(resolve => chrome.once('close', resolve));
    chrome.kill();
    await closed;
  }
  await rm(profile, { recursive: true, force: true, maxRetries: 5 });
}
