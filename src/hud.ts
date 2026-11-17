import './app.css';
import { mount } from 'svelte';
import Hud from './Hud.svelte';

export default mount(Hud, { target: document.getElementById('app')! });
