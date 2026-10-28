import React from 'react';
import { createRoot } from 'react-dom/client';
import { Dashboard } from './views/Dashboard';
import { Overlay } from './views/Overlay';
import './styles.css';

const overlay = new URLSearchParams(window.location.search).get('view') === 'overlay';
document.documentElement.dataset.view = overlay ? 'overlay' : 'dashboard';
createRoot(document.getElementById('root')!).render(<React.StrictMode>{overlay ? <Overlay /> : <Dashboard />}</React.StrictMode>);
