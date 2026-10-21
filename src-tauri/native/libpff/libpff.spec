Name: libpff
Version: 20260625
Release: 1
Summary: Library to access the Personal Folder File (OST, PAB and PST) format
Group: System Environment/Libraries
License: LGPL-3.0-or-later
Source: %{name}-%{version}.tar.gz
URL: https://github.com/libyal/libpff
Requires:                  zlib
BuildRequires: gcc                  zlib-devel

%description -n libpff
Library to access the Personal Folder File (OST, PAB and PST) format

%package -n libpff-static
Summary: Library to access the Personal Folder File (OST, PAB and PST) format
Group: Development/Libraries
Requires: libpff = %{version}-%{release}

%description -n libpff-static
Static library version of libpff.

%package -n libpff-devel
Summary: Header files and libraries for developing applications for libpff
Group: Development/Libraries
Requires: libpff = %{version}-%{release}

%description -n libpff-devel
Header files and libraries for developing applications for libpff.

%package -n libpff-python3
Summary: Python 3 bindings for libpff
Group: System Environment/Libraries
Requires: libpff = %{version}-%{release} python3
BuildRequires: python3-devel python3-setuptools

%description -n libpff-python3
Python 3 bindings for libpff

%package -n libpff-tools
Summary: Several tools for reading Personal Folder Files (OST, PAB and PST)
Group: Applications/System
Requires: libpff = %{version}-%{release}

%description -n libpff-tools
Several tools for reading Personal Folder Files (OST, PAB and PST)

%prep
%setup -q

%build
%configure --prefix=/usr --libdir=%{_libdir} --mandir=%{_mandir} --enable-python
make %{?_smp_mflags}

%install
rm -rf %{buildroot}
%make_install

%clean
rm -rf %{buildroot}

%post -p /sbin/ldconfig

%postun -p /sbin/ldconfig

%files -n libpff
%license COPYING COPYING.LESSER
%doc AUTHORS README
%{_libdir}/*.so.*

%files -n libpff-static
%license COPYING COPYING.LESSER
%doc AUTHORS README
%{_libdir}/*.a

%files -n libpff-devel
%license COPYING COPYING.LESSER
%doc AUTHORS README
%{_libdir}/*.so
%{_libdir}/pkgconfig/libpff.pc
%{_includedir}/*
%{_mandir}/man3/*

%files -n libpff-python3
%license COPYING COPYING.LESSER
%doc AUTHORS README
%{_libdir}/python3*/site-packages/*.a
%{_libdir}/python3*/site-packages/*.so

%files -n libpff-tools
%license COPYING COPYING.LESSER
%doc AUTHORS README
%{_bindir}/*
%{_mandir}/man1/*

%changelog
* Thu Aug 20 2026 Joachim Metz <joachim.metz@gmail.com> 20260625-1
- Auto-generated

