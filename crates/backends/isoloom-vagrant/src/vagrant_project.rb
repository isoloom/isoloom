# The project goes into a VM through Isoloom's own `project` step (the `isoloom_project`
# provisioner below): a tar.gz of the project as it is when the step runs, not when this file
# loads, so files added or removed since still count. Symbolic links go in as links, never
# followed (a link loop can't break the copy). Plain Ruby (Vagrant's own): no tool on the host.
require "zlib"
require "tempfile"

module IsoloomProject
  # Writes the archive of `root` to `io`, gzipped, leaving out what `.isoloomignore` lists, then
  # what `rules` (lines of its syntax, read after the file's) list: Isoloom's outputs but those
  # the machine reads, version control and the tools' state (`COPY_RULES`, the generator's).
  def self.write(root, io, rules: COPY_RULES)
    gz = Zlib::GzipWriter.new(io)
    rules = ignore_rules(root, rules)
    walk(root, nil, rules) { |rel, st| entry(gz, root, rel, st) }
    gz.write("\0" * 1024)
    gz.finish
  end

  # `.isoloomignore` at the project's root, gitignore-style: one pattern per line, `#` comments,
  # `!` brings a path back, a trailing `/` matches folders only, a `/` at the start or in the
  # middle anchors the pattern at the root (otherwise it matches at any depth), `*`, `?`,
  # `[abc]` and `**`. A pattern matching a folder matches everything in it, and the last pattern
  # matching a path (or a folder it is in) decides. As [regexp, keep] pairs, `copy` read after
  # the file; the Terraform outputs translate the patterns the same way, so every target leaves
  # out the same files.
  def self.ignore_rules(root, copy)
    text = begin
      File.binread(File.join(root, ".isoloomignore")).force_encoding("UTF-8")
    rescue Errno::ENOENT
      ""
    end
    "#{text}\n#{copy}".split("\n").filter_map do |line|
      line = line.strip
      next if line.empty? || line.start_with?("#")
      keep = line.start_with?("!")
      pat = keep ? line[1..] : line
      glob = pat.delete_suffix("/").delete_suffix("/**").delete_suffix("/")
      next if glob.empty?
      anchored = glob.include?("/") ? glob.delete_prefix("/") : "**/#{glob}"
      re = anchored.gsub(/[.+^$(){}|\\]/) { "\\#{$&}" }.gsub("[!", "[^")
                   .gsub("**/", "\u0001").gsub("**", "\u0002").gsub("*", "[^/]*").gsub("?", "[^/]")
                   .gsub("\u0001", "(.*/)?").gsub("\u0002", ".*")
      [Regexp.new(glob == pat ? "^#{re}(/|$)" : "^#{re}/"), keep]
    end
  end

  # Whether the rules leave a path out (a folder's path ends with `/`).
  def self.ignored?(rules, path)
    rules.reduce(false) { |out, (re, keep)| re.match?(path) ? !keep : out }
  end

  # Every entry under `dir` (relative to `root`), parents first, in name order.
  def self.walk(root, dir, rules, &block)
    names = begin
      Dir.children(dir ? File.join(root, dir) : root).sort
    rescue Errno::ENOENT, Errno::ENOTDIR
      return # removed while the archive was being written
    end
    names.each do |name|
      rel = dir ? "#{dir}/#{name}" : name
      st = begin
        File.lstat(File.join(root, rel))
      rescue Errno::ENOENT
        next
      end
      folder = st.directory?
      out = ignored?(rules, folder ? "#{rel}/" : rel)
      yield rel, st unless out
      # An ignored folder is skipped, unless a `!` pattern may bring back something in it.
      walk(root, rel, rules, &block) if folder && (!out || rules.any? { |_, keep| keep })
    end
  end

  def self.entry(out, root, rel, st)
    path = File.join(root, rel)
    if st.symlink?
      header(out, rel, "2", 0o777, 0, st.mtime, File.readlink(path))
    elsif st.directory?
      header(out, "#{rel}/", "5", st.mode & 0o7777, 0, st.mtime)
    elsif st.file?
      File.open(path, "rb") do |f|
        mode = st.mode & 0o7777
        # Windows has no executable bit: a script (#!) is made executable.
        mode |= 0o111 if Gem.win_platform? && f.read(2) == "#!"
        f.rewind
        header(out, rel, "0", mode, st.size, st.mtime)
        copied = IO.copy_stream(f, out, st.size)
        out.write("\0" * (st.size - copied)) if copied < st.size # shrank while being read
        out.write("\0" * ((512 - st.size % 512) % 512))
      end
    end
  rescue Errno::ENOENT
    nil # removed while the archive was being written
  end

  # A ustar header; a pax one before it for what ustar can't hold (long names, large sizes).
  def self.header(out, name, type, mode, size, mtime, link = "")
    name = name.b
    link = link.b
    pax = {}
    pax["path"] = name if name.bytesize > 100
    pax["linkpath"] = link if link.bytesize > 100
    pax["size"] = size.to_s if size >= 8**11
    unless pax.empty?
      body = pax.map { |k, v| record(k, v) }.join
      header(out, "PaxHeader/#{name.byteslice(0, 80)}", "x", 0o644, body.bytesize, mtime)
      out.write(body)
      out.write("\0" * ((512 - body.bytesize % 512) % 512))
    end
    h = [
      name.byteslice(0, 100), format("%07o", mode), format("%07o", 0), format("%07o", 0),
      format("%011o", size >= 8**11 ? 0 : size), format("%011o", [mtime.to_i, 0].max), " " * 8, type,
      link.byteslice(0, 100), "ustar", "00", "", "", "", "", "", "",
    ].pack("a100a8a8a8a12a12a8a1a100a6a2a32a32a8a8a155a12")
    h[148, 8] = format("%06o\0 ", h.sum(0))
    out.write(h)
  end

  # One pax record: "<length> key=value\n", the length counting itself.
  def self.record(key, value)
    rest = " #{key}=".b + value + "\n"
    len = rest.bytesize + 1
    len += 1 while (len.to_s.bytesize + rest.bytesize) > len
    len.to_s.b + rest
  end
end

if defined?(Vagrant) && Vagrant.respond_to?(:plugin)
  module IsoloomProject
    class Config < Vagrant.plugin("2", :config)
      attr_accessor :name, :rules

      def initialize
        super
        @name = UNSET_VALUE
        @rules = UNSET_VALUE
      end

      def finalize!
        @name = nil if @name == UNSET_VALUE
        @rules = COPY_RULES if @rules == UNSET_VALUE
      end
    end

    # Builds the archive, uploads it to LINUX_ARCHIVE (WINDOWS_ARCHIVE on Windows, over WinRM or
    # OpenSSH) and unpacks it as root with LINUX_UNPACK (WINDOWS_UNPACK), replacing an older copy.
    # The archives' paths and the unpacking are the generator's, set after this file.
    class Provisioner < Vagrant.plugin("2", :provisioner)
      def provision
        windows = %w[winrm winssh].include?(@machine.config.vm.communicator.to_s)
        Tempfile.create(["isoloom-project", ".tgz"]) do |f|
          f.binmode
          IsoloomProject.write(ROOT, f, rules: config.rules)
          f.close
          @machine.ui.info("The project, #{(File.size(f.path) / 1024.0).ceil} KB compressed, into the VM")
          comm = @machine.communicate
          if windows
            comm.upload(f.path, WINDOWS_ARCHIVE)
            comm.execute(WINDOWS_UNPACK, elevated: true) do |type, data|
              @machine.ui.info(data.chomp) if type == :stdout && !data.strip.empty?
            end
          else
            comm.upload(f.path, LINUX_ARCHIVE)
            comm.sudo(LINUX_UNPACK)
          end
        end
      end
    end

    class Plugin < Vagrant.plugin("2")
      name "isoloom project"
      config(:isoloom_project, :provisioner) { Config }
      provisioner(:isoloom_project) { Provisioner }
    end
  end
end
