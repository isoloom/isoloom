# The project goes into a VM through Isoloom's own `project` step (the `isoloom_project`
# provisioner below): a tar.gz of the project as it is when the step runs, not when this file
# loads, so files added or removed since still count. Symbolic links go in as links, never
# followed (a link loop can't break the copy). Plain Ruby (Vagrant's own): no tool on the host.
require "zlib"
require "tempfile"

module IsoloomProject
  # Never copied, at any depth: version control and the tools' own state.
  SKIP = [".git", ".vagrant", ".terraform"].freeze

  # Writes the archive of `root` to `io`, gzipped. `generated`: whether Isoloom's outputs (the
  # top-level `.isoloom*` folders) go in; `extra`: files in them that go in anyway (paths from
  # `root`, with `/`).
  def self.write(root, io, generated: false, extra: [])
    gz = Zlib::GzipWriter.new(io)
    walk(root, nil, generated, extra) { |rel, st| entry(gz, root, rel, st) }
    gz.write("\0" * 1024)
    gz.finish
  end

  # Every entry under `dir` (relative to `root`), parents first, in name order.
  def self.walk(root, dir, generated, extra, &block)
    names = begin
      Dir.children(dir ? File.join(root, dir) : root).sort
    rescue Errno::ENOENT, Errno::ENOTDIR
      return # removed while the archive was being written
    end
    names.each do |name|
      next if SKIP.include?(name)
      rel = dir ? "#{dir}/#{name}" : name
      if !generated && rel.start_with?(".isoloom")
        next unless extra.any? { |e| e == rel || e.start_with?("#{rel}/") }
      end
      st = begin
        File.lstat(File.join(root, rel))
      rescue Errno::ENOENT
        next
      end
      yield rel, st
      walk(root, rel, generated, extra, &block) if st.directory?
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
      attr_accessor :name, :generated, :extra

      def initialize
        super
        @name = UNSET_VALUE
        @generated = UNSET_VALUE
        @extra = UNSET_VALUE
      end

      def finalize!
        @name = nil if @name == UNSET_VALUE
        @generated = false if @generated == UNSET_VALUE
        @extra = [] if @extra == UNSET_VALUE
      end
    end

    # Builds the archive, uploads it, unpacks it in /opt/isoloom (replacing an older copy).
    class Provisioner < Vagrant.plugin("2", :provisioner)
      def provision
        Tempfile.create(["isoloom-project", ".tgz"]) do |f|
          f.binmode
          IsoloomProject.write(ROOT, f, generated: config.generated, extra: config.extra)
          f.close
          @machine.ui.info("The project, #{(File.size(f.path) / 1024.0).ceil} KB compressed, to /opt/isoloom")
          comm = @machine.communicate
          comm.upload(f.path, "/tmp/isoloom-project.tgz")
          comm.execute("rm -rf /tmp/isoloom-project && mkdir /tmp/isoloom-project && tar -xzf /tmp/isoloom-project.tgz -C /tmp/isoloom-project && rm -f /tmp/isoloom-project.tgz")
          comm.sudo("rm -rf /opt/isoloom && mv /tmp/isoloom-project /opt/isoloom")
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
