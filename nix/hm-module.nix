self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.pomolog;
  toml = pkgs.formats.toml { };
in
{
  options.programs.pomolog = {
    enable = lib.mkEnableOption "Pomolog, a pomodoro timer with kanban, calendar and habits";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "pomolog.packages.\${system}.default";
      description = "The Pomolog package to use.";
    };

    autostart = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Start Pomolog with the graphical session, so the tray icon and CLI are always available.";
    };

    settings = lib.mkOption {
      type = lib.types.nullOr toml.type;
      default = null;
      example = lib.literalExpression ''
        {
          timer = { focus_min = 50; short_break_min = 10; daily_goal = 6; };
          appearance = { theme = "system"; mode = "dark"; };
          overrides.accent = "#ff8800";
        }
      '';
      description = ''
        Written to `~/.config/pomolog/config.toml`. When set, the file is
        read-only and changes made in the app's settings are not saved.
        Leave unset to manage settings from inside the app.
      '';
    };

    themes = lib.mkOption {
      type = lib.types.attrsOf toml.type;
      default = { };
      example = lib.literalExpression ''
        {
          mine = {
            inherit = "system";
            colors.focus = "#ff8800";
          };
        }
      '';
      description = "Theme files, written to `~/.config/pomolog/themes/<name>.toml`.";
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];

    xdg.configFile =
      lib.optionalAttrs (cfg.settings != null) {
        "pomolog/config.toml".source = toml.generate "pomolog-config.toml" cfg.settings;
      }
      // lib.mapAttrs' (
        name: value:
        lib.nameValuePair "pomolog/themes/${name}.toml" {
          source = toml.generate "pomolog-theme-${name}.toml" value;
        }
      ) cfg.themes;

    systemd.user.services.pomolog = lib.mkIf cfg.autostart {
      Unit = {
        Description = "Pomolog";
        PartOf = [ "graphical-session.target" ];
        After = [ "graphical-session.target" ];
      };
      Service = {
        ExecStart = lib.getExe cfg.package;
        Restart = "on-failure";
      };
      Install.WantedBy = [ "graphical-session.target" ];
    };
  };
}
