// Claude HUD Float — Frontend Entry Point
//
// This is the first JS module loaded by the browser. Its only job is to:
// 1. Import the CSS (Vite handles bundling and HMR injection).
// 2. Import and initialize the app module on DOMContentLoaded.

import './styles.css';
import { initApp } from './app';

document.addEventListener('DOMContentLoaded', initApp);
