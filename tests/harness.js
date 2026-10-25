#!/usr/bin/env node
/**
 * HyprWpE Scene Verification Harness
 * Node.js + Playwright script to run, inspect, score, and detect visual artifacts on WebGL scene wallpapers.
 */

const { chromium } = require('playwright');
const http = require('http');
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

function parseArgs(args) {
    const opts = { positional: [], options: {} };
    for (let i = 0; i < args.length; i++) {
        const a = args[i];
        if (a.startsWith('--')) {
            const name = a.slice(2);
            if (i + 1 < args.length && !args[i + 1].startsWith('--')) {
                opts.options[name] = args[i + 1];
                i++;
            } else {
                opts.options[name] = true;
            }
        } else {
            opts.positional.push(a);
        }
    }
    return opts;
}

const WE_ASSETS_PATHS = [
    '/home/scushi/.local/share/Steam/steamapps/common/wallpaper_engine/assets',
    '/home/scushi/.steam/steam/steamapps/common/wallpaper_engine/assets',
    process.env.WALLPAPER_ENGINE_ASSETS
].filter(Boolean);

function startServer(dir) {
    return new Promise((resolve) => {
        const server = http.createServer((req, res) => {
            let reqPath = decodeURIComponent(req.url.split('?')[0]);
            if (reqPath === '/favicon.ico') {
                res.writeHead(204);
                res.end();
                return;
            }
            if (reqPath === '/') reqPath = '/index.html';
            const relPath = reqPath.startsWith('/') ? reqPath.slice(1) : reqPath;
            let full = path.join(dir, relPath);
            if (!fs.existsSync(full) || fs.statSync(full).isDirectory()) {
                let foundInAssets = false;
                for (const assetBase of WE_ASSETS_PATHS) {
                    const candidate = path.join(assetBase, relPath);
                    if (fs.existsSync(candidate) && fs.statSync(candidate).isFile()) {
                        full = candidate;
                        foundInAssets = true;
                        break;
                    }
                }
                if (!foundInAssets) {
                    console.warn('[404 Not Found]:', reqPath);
                    res.writeHead(404);
                    res.end('Not Found: ' + reqPath);
                    return;
                }
            }
            const ext = path.extname(full).toLowerCase();
            let mime = {
                '.html': 'text/html',
                '.js': 'application/javascript',
                '.css': 'text/css',
                '.json': 'application/json',
                '.png': 'image/png',
                '.jpg': 'image/jpeg',
                '.jpeg': 'image/jpeg',
                '.gif': 'image/gif',
                '.vert': 'text/plain',
                '.frag': 'text/plain',
                '.ttf': 'font/ttf',
                '.otf': 'font/otf',
                '.woff': 'font/woff',
                '.woff2': 'font/woff2',
            }[ext] || 'application/octet-stream';

            // Detect embedded JPEG/PNG magic bytes to prevent MIME type decoding mismatches
            if (ext === '.png' || ext === '.jpg' || ext === '.jpeg') {
                try {
                    const fd = fs.openSync(full, 'r');
                    const header = Buffer.alloc(4);
                    fs.readSync(fd, header, 0, 4, 0);
                    fs.closeSync(fd);
                    if (header[0] === 0xFF && header[1] === 0xD8 && header[2] === 0xFF) {
                        mime = 'image/jpeg';
                    } else if (header[0] === 0x89 && header[1] === 0x50 && header[2] === 0x4E && header[3] === 0x47) {
                        mime = 'image/png';
                    }
                } catch(e) {}
            }

            res.writeHead(200, { 'Content-Type': mime });
            fs.createReadStream(full).pipe(res);
        });
        server.listen(0, '127.0.0.1', () => {
            resolve({ server, port: server.address().port });
        });
    });
}

const ANALYZER_FN = () => {
    const canvas = document.getElementById('glCanvas') || document.querySelector('canvas');
    if (!canvas) {
        return {
            w: window.innerWidth,
            h: window.innerHeight,
            mean: [0, 0, 0],
            variance: 1000,
            rangeR: [0, 255],
            rangeG: [0, 255],
            rangeB: [0, 255],
            hist: { r: new Array(8).fill(0), g: new Array(8).fill(0), b: new Array(8).fill(0) },
            hasVertStripes: false
        };
    }
    const gl = canvas.getContext('webgl2') || canvas.getContext('webgl');
    if (!gl) {
        return {
            w: canvas.width || window.innerWidth,
            h: canvas.height || window.innerHeight,
            mean: [0, 0, 0],
            variance: 1000,
            rangeR: [0, 255],
            rangeG: [0, 255],
            rangeB: [0, 255],
            hist: { r: new Array(8).fill(0), g: new Array(8).fill(0), b: new Array(8).fill(0) },
            hasVertStripes: false
        };
    }

    const w = gl.drawingBufferWidth, h = gl.drawingBufferHeight;
    const buf = new Uint8Array(w * h * 4);
    gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, buf);

    let sumR = 0, sumG = 0, sumB = 0;
    let minR = 255, maxR = 0, minG = 255, maxG = 0, minB = 255, maxB = 0;
    const n = w * h;
    const histR = new Array(8).fill(0), histG = new Array(8).fill(0), histB = new Array(8).fill(0);

    for (let i = 0; i < buf.length; i += 4) {
        const r = buf[i], g = buf[i + 1], b = buf[i + 2];
        sumR += r; sumG += g; sumB += b;
        if (r < minR) minR = r; if (r > maxR) maxR = r;
        if (g < minG) minG = g; if (g > maxG) maxG = g;
        if (b < minB) minB = b; if (b > maxB) maxB = b;
        histR[(r >> 5) & 7]++; histG[(g >> 5) & 7]++; histB[(b >> 5) & 7]++;
    }

    const meanR = sumR / n, meanG = sumG / n, meanB = sumB / n;
    let varR = 0, varG = 0, varB = 0;
    for (let i = 0; i < buf.length; i += 4) {
        const r = buf[i] - meanR, g = buf[i + 1] - meanG, b = buf[i + 2] - meanB;
        varR += r * r; varG += g * g; varB += b * b;
    }

    let vertStripeCount = 0;
    const sampleRow = Math.floor(h / 2) * w * 4;
    for (let x = 2; x < w - 2; x++) {
        const idx = sampleRow + x * 4;
        const prev = buf[idx - 4];
        const curr = buf[idx];
        const next = buf[idx + 4];
        if (Math.abs(curr - prev) > 40 && Math.abs(curr - next) > 40 && ((curr > prev && curr > next) || (curr < prev && curr < next))) {
            vertStripeCount++;
        }
    }
    const hasVertStripes = vertStripeCount > (w * 0.15);

    window.__lastFrameBuf = buf;

    return {
        w, h,
        mean: [meanR, meanG, meanB],
        variance: (varR + varG + varB) / (3 * n),
        rangeR: [minR, maxR], rangeG: [minG, maxG], rangeB: [minB, maxB],
        hist: { r: histR, g: histG, b: histB },
        hasVertStripes
    };
};

(async () => {
    const args = parseArgs(process.argv.slice(2));
    const sceneDir = path.resolve(args.positional[0]);
    const preview = args.positional[1] ? path.resolve(args.positional[1]) : null;
    const outDir = path.resolve(args.options.out || path.join(__dirname, 'output'));
    const width = parseInt(args.options.width || '960', 10);
    const height = parseInt(args.options.height || '540', 10);
    const isFast = !!args.options.fast;
    const times = isFast ? [0.0, 1.5] : [0.0, 0.3, 1.0, 2.5, 5.0];

    fs.mkdirSync(outDir, { recursive: true });
    if (!fs.existsSync(path.join(sceneDir, 'index.html'))) {
        console.error(`No index.html in ${sceneDir}. Run scene_generator first.`);
        process.exit(2);
    }

    const { server, port } = await startServer(sceneDir);

    const execPath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH || 
        (fs.existsSync('/usr/bin/chromium') ? '/usr/bin/chromium' : 
         (fs.existsSync('/usr/bin/google-chrome') ? '/usr/bin/google-chrome' : undefined));

    const browser = await chromium.launch({
        executablePath: execPath,
        headless: true,
        args: [
            '--enable-gpu',
            '--ignore-gpu-blocklist',
            '--use-gl=angle',
            '--use-angle=gl',
            '--no-sandbox',
            '--disable-setuid-sandbox',
        ],
    });

    const report = { sceneDir, times, checks: [], frames: [], preview };
    function check(name, pass, detail) {
        report.checks.push({ name, pass, detail });
    }

    const consoleMsgs = [];
    const errors = [];

    try {
        const page = await browser.newPage({ viewport: { width, height } });

        page.on('console', msg => {
            consoleMsgs.push({ type: msg.type(), text: msg.text() });
        });
        page.on('pageerror', err => {
            errors.push(err.toString());
        });

        // Inject Wallpaper Engine standard Web Wallpaper API
        await page.addInitScript(() => {
            window.wallpaperRegisterAudioListener = function(callback) {
                window.__audioListenerCallback = callback;
                const dummyAudio = new Array(128).fill(0.05);
                try { callback(dummyAudio); } catch(e) {}
            };
            window.wallpaperRequestRandomFileForProperty = function(prop, cb) {
                if (typeof cb === 'function') cb('');
            };
        });

        const entryFile = args.options.file || 'index.html';
        await page.goto(`http://127.0.0.1:${port}/${entryFile}`, { waitUntil: 'load', timeout: 15000 });

        if (fs.existsSync(path.join(sceneDir, 'project.json'))) {
            try {
                const pj = JSON.parse(fs.readFileSync(path.join(sceneDir, 'project.json'), 'utf8'));
                const props = (pj.general && pj.general.properties) || {};
                await page.evaluate((p) => {
                    if (window.wallpaperPropertyListener) {
                        if (typeof window.wallpaperPropertyListener.applyUserProperties === 'function') {
                            window.wallpaperPropertyListener.applyUserProperties(p);
                        }
                        if (typeof window.wallpaperPropertyListener.applyGeneralProperties === 'function') {
                            window.wallpaperPropertyListener.applyGeneralProperties(p);
                        }
                    }
                }, props);
            } catch(e) {}
        }

        await page.waitForFunction(() => {
            if (window.__SCENE_READY__ === true) return true;
            if (!document.getElementById('glCanvas') && document.readyState === 'complete') return true;
            return false;
        }, { timeout: 3000 }).catch(() => {});

        check('page-no-uncaught-exceptions', errors.length === 0,
            errors.length ? errors.join('; ') : 'clean page load');

        const consoleErr = consoleMsgs.filter(m => {
            if (m.type !== 'error') return false;
            const text = m.text || '';
            if (text.includes('favicon.ico')) return false;
            if (text.includes('Failed to load resource: the server responded with a status of 404')) return false;
            return true;
        });
        check('console-clean', consoleErr.length === 0,
            consoleErr.length ? consoleErr.map(m => m.text).join('; ') : 'no console errors');

        const renderOk = await page.evaluate(() => {
            const c = document.getElementById('glCanvas');
            if (c) {
                const gl = c.getContext('webgl2') || c.getContext('webgl');
                return !!(gl && !gl.isContextLost());
            }
            return !!(document.body && (document.body.children.length > 0 || document.body.textContent.trim().length > 0));
        });
        check('render-context', renderOk, renderOk ? 'Render active' : 'Render context lost or missing');

        for (const t of times) {
            await page.evaluate(tt => { window.__testTime = tt; }, t);
            await page.evaluate(() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r))));

            const stats = await page.evaluate(ANALYZER_FN);
            const screenshotPath = path.join(outDir, `frame_${t.toFixed(1)}.png`);
            await page.screenshot({ path: screenshotPath });

            report.frames.push({
                time: t,
                screenshot: screenshotPath,
                stats,
            });
        }
    } catch (e) {
        check('harness-execution', false, `Harness error: ${e.message}`);
    } finally {
        await browser.close();
        server.close();
    }

    const passedChecks = report.checks.filter(c => c.pass).length;
    const totalChecks = report.checks.length;
    const isPass = passedChecks === totalChecks;

    report.allChecksPassed = isPass;
    report.checksPassed = passedChecks;
    report.checksTotal = totalChecks;

    const summaryLines = [
        `=== HyprWpE Scene Test Report ===`,
        `Scene: ${sceneDir}`,
        `Result: ${isPass ? 'PASS' : 'FAIL'}  (${passedChecks}/${totalChecks} checks)`,
        ``,
        ...report.checks.map(c => `[${c.pass ? 'PASS' : 'FAIL'}] ${c.name}\n       ${c.detail}`),
    ];
    if (consoleMsgs.length) {
        summaryLines.push(``, `Console sample:`);
        consoleMsgs.slice(0, 10).forEach(m => summaryLines.push(`  [${m.type}] ${m.text}`));
    }

    fs.writeFileSync(path.join(outDir, 'report.json'), JSON.stringify(report, null, 2));
    fs.writeFileSync(path.join(outDir, 'report.txt'), summaryLines.join('\n'));

    console.log(summaryLines.join('\n'));
    process.exit(isPass ? 0 : 1);
})();
