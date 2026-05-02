const fs = require('fs');
const html = fs.readFileSync('ui.html', 'utf8');
const scriptMatches = html.matchAll(/<script.*?>([\s\S]*?)<\/script>/g);
let i = 0;
for (const match of scriptMatches) {
    i++;
    try {
        new Function(match[1]);
        console.log(`Script ${i} is valid`);
    } catch (e) {
        console.log(`Script ${i} syntax error:`, e.message);
    }
}
