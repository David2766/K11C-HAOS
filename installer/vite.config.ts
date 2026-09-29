import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
export default defineConfig({plugins:[vue()],clearScreen:false,server:{strictPort:true,watch:{ignored:['**/target/**','**/release/**','**/test-results/**']}},build:{target:'es2022'}});
