# CanvasTUI

This is a tool meant for the terminal that allows me to view my upcoming assignments on [canvas](https://www.instructure.com/canvas) faster.

The tool is designed specifically for me personally, but it's also possible to repurpose it for your own canvas account as well.

## Prerequisites
- The app uses environment variables to get your [Canvas Access Key](https://community.canvaslms.com/t5/Admin-Guide/How-do-I-manage-API-access-tokens-as-an-admin/ta-p/89)
- Store the Canvas Access Token in the environment variable **CANVAS_ACCESS_TOKEN** (for example add this to your .bashrc file):
```bash
export CANVAS_ACCESS_TOKEN="key-here"
```
- Store the Base Canvas URL in the environment variable **CANVAS_URL** (for example add this to your .bashrc file):
```bash
export CANVAS_URL="https://canvas.csuchico.edu"
``````

## Controls
I based the controls on Vim bindings as a Neovim user. Here are the current supported keybinds:
- `j`: Move down
- `k`: Move up
- `h`: Go to previous day
- `l`: Go to next day
- `o`: Open the url in your browser
- `q`: Quit the app
